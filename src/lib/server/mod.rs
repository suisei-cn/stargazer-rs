use std::fmt::Debug;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use actix::{Arbiter, SystemService};
use actix_web::dev::Server as WebServer;
use actix_web::middleware::{Logger, NormalizePath};
use actix_web::web::{Data, ServiceConfig};
use actix_web::App;
use actix_web::HttpServer;
use pin_project::pin_project;
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use handler::ArbiterHandler;
pub use killer::KillerActor;
use killer::RegisterHttpServer;

use crate::context::{ArbiterContext, InstanceContext};
use crate::server::watchdog::WatchdogActor;
use crate::HTTPConfig;

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

impl From<HTTPConfig> for ServerMode {
    fn from(config: HTTPConfig) -> Self {
        match config {
            HTTPConfig::Disabled => Self::NoHTTP,
            HTTPConfig::Enabled { host, port } => Self::HTTP {
                port: SocketAddr::from((host, port)),
            },
        }
    }
}

pub struct Server<F, SF>
where
    F: Fn(Uuid) -> (ArbiterContext, SF) + Send + Sync + Clone + 'static,
    SF: 'static + FnOnce(&mut ServiceConfig),
{
    factory: F,
    workers: usize,
    instance_ctx: InstanceContext,
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
            instance_ctx: ctx,
        }
    }

    /// Set number of workers to start.
    ///
    /// By default, server uses number of available logical CPU as thread count.
    pub fn workers(mut self, num: impl Into<Option<usize>>) -> Self {
        self.workers = num.into().map_or_else(num_cpus::get, |num| num);
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
        let instance_id = self.instance_ctx.id();
        let instance_ctx = Arc::new(self.instance_ctx);
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
                        ArbiterContext::set(ctx.clone());
                        instance_ctx.register(ctx);
                    });
                };
                for _ in 0..self.workers {
                    (arb_factory.clone())();
                }

                KillerActor::from_registry();
                Ok(ServerHandler::NoHTTP(ArbiterHandler::new(
                    self.workers,
                    stop_rx,
                )))
            }
            ServerMode::HTTP { port } => {
                let srv = HttpServer::new(move || {
                    let instance_ctx = instance_ctx.clone();
                    let (ctx, http_services) = factory(instance_id);
                    ArbiterContext::set(ctx.clone());
                    instance_ctx.register(ctx.clone());
                    App::new()
                        .wrap(Logger::default())
                        .wrap(NormalizePath::trim())
                        .app_data(Data::new(ctx))
                        .app_data(Data::from(instance_ctx))
                        .configure(http_services)
                })
                .workers(self.workers)
                .bind(port)?
                .run();

                KillerActor::from_registry().do_send(RegisterHttpServer::new(srv.handle()));
                Ok(ServerHandler::HTTP(srv))
            }
        }
    }
}
