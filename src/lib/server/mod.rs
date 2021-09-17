use std::fmt::Debug;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use actix::Arbiter;
use actix_web::dev::Server as WebServer;
use actix_web::middleware::Logger;
use actix_web::web::{Data, ServiceConfig};
use actix_web::{App, HttpServer};
use parking_lot::RwLock;
use pin_project::pin_project;
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use handler::ArbiterHandler;
pub use killer::KillerActor;

use crate::context::{ArbiterContext, InstanceContext};
use crate::server::watchdog::WatchdogActor;

mod handler;
mod killer;
mod watchdog;

#[cfg(test)]
mod tests;

#[pin_project(project = ServerHandlerProj)]
pub enum ServerHandler {
    NoHTTP(#[pin] ArbiterHandler),
    HTTP(#[pin] WebServer),
}

impl Future for ServerHandler {
    type Output = std::io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this: ServerHandlerProj = self.project();
        match this {
            ServerHandlerProj::NoHTTP(handler) => {
                let handler: Pin<&mut ArbiterHandler> = handler;
                handler.poll(cx).map(|_| Ok(()))
            }
            ServerHandlerProj::HTTP(srv) => {
                let srv: Pin<&mut WebServer> = srv;
                srv.poll(cx)
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ServerMode {
    NoHTTP,
    HTTP { port: SocketAddr },
}

pub struct Server<F, SF>
where
    F: Fn(Uuid) -> (ArbiterContext, SF) + Send + Sync + Clone + 'static,
    SF: 'static + FnOnce(&mut ServiceConfig),
{
    factory: F,
    workers: usize,
    instance_ctx: Arc<RwLock<InstanceContext>>,
}

impl<F, SF> Server<F, SF>
where
    F: Fn(Uuid) -> (ArbiterContext, SF) + Send + Sync + Clone + 'static,
    SF: 'static + FnOnce(&mut ServiceConfig),
{
    /// Create a new server instance.
    ///
    /// F: `instance_id` -> (`ArbiterContext`, `HttpServices`)
    pub fn new(factory: F) -> Self {
        let ctx = InstanceContext::new();
        Self {
            factory,
            workers: num_cpus::get(),
            instance_ctx: Arc::new(RwLock::new(ctx)),
        }
    }

    /// Set number of workers to start.
    ///
    /// By default, server uses number of available logical CPU as thread count.
    pub fn workers(mut self, num: usize) -> Self {
        self.workers = num;
        self
    }
}

impl<F, SF> Server<F, SF>
where
    F: Fn(Uuid) -> (ArbiterContext, SF) + Send + Sync + Clone + 'static,
    SF: 'static + FnOnce(&mut ServiceConfig),
{
    /// Start the server.
    ///
    /// This function will return a [`ServerHandler`](ServerHandler).
    /// You may want to `await` it to block on the server.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if error occur when binding the port.
    pub fn run(self, mode: ServerMode) -> std::io::Result<ServerHandler> {
        let instance_id = self.instance_ctx.read().id();
        let instance_ctx = self.instance_ctx;
        let factory = self.factory;

        match mode {
            ServerMode::NoHTTP => {
                let (stop_tx, stop_rx) = unbounded_channel();

                let arb_factory = move || {
                    let tx = stop_tx.clone();

                    let arb = Arbiter::new();
                    arb.spawn_fn(move || {
                        let (ctx, _) = factory(instance_id);
                        WatchdogActor::start(tx);
                        instance_ctx.write().register(ctx);
                    });
                };
                for _ in 0..self.workers {
                    (arb_factory.clone())();
                }

                KillerActor::start(None);
                Ok(ServerHandler::NoHTTP(ArbiterHandler::new(
                    self.workers,
                    stop_rx,
                )))
            }
            ServerMode::HTTP { port } => {
                let srv = HttpServer::new(move || {
                    let (ctx, http_services) = factory(instance_id);
                    instance_ctx.write().register(ctx.clone());
                    App::new()
                        .wrap(Logger::default())
                        .app_data(Data::new(ctx))
                        .app_data(Data::new(instance_ctx.read().clone()))
                        .configure(http_services)
                })
                .workers(self.workers)
                .bind(port)?
                .run();

                KillerActor::start(Some(srv.clone()));
                Ok(ServerHandler::HTTP(srv))
            }
        }
    }
}
