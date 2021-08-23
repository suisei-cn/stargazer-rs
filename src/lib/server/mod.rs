use std::fmt::Debug;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use actix::{Arbiter, SystemService};
use actix_web::dev::Server as WebServer;
use actix_web::middleware::Logger;
use actix_web::web::{Data, ServiceConfig};
use actix_web::App;
use actix_web::HttpServer;
use parking_lot::RwLock;
use pin_project::pin_project;
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use handler::ArbiterHandler;
pub use killer::KillerActor;
use killer::RegisterHttpServer;

use crate::context::{ArbiterContext, InstanceContext};
use crate::error::Result;
use crate::server::watchdog::WatchdogActor;

mod handler;
mod killer;
mod watchdog;

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
    instance_ctx: Arc<RwLock<InstanceContext>>,
}

impl<F, SF> Server<F, SF>
where
    F: Fn(Uuid) -> (ArbiterContext, SF) + Send + Sync + Clone + 'static,
    SF: 'static + FnOnce(&mut ServiceConfig),
{
    /// Create a new server instance.
    ///
    /// F: instance_id -> (ArbiterContext, HttpServices)
    pub fn new(factory: F) -> Self {
        let ctx = InstanceContext::new();
        Server {
            factory,
            instance_ctx: Arc::new(RwLock::new(ctx)),
        }
    }
}

impl<F, SF> Server<F, SF>
where
    F: Fn(Uuid) -> (ArbiterContext, SF) + Send + Sync + Clone + 'static,
    SF: 'static + FnOnce(&mut ServiceConfig),
{
    pub fn run(self, mode: ServerMode) -> Result<ServerHandler> {
        let instance_id = self.instance_ctx.read().id();

        match mode {
            ServerMode::NoHTTP => {
                let (tx, rx) = unbounded_channel();

                let cpus = num_cpus::get();
                for _ in 0..cpus {
                    let instance_ctx = self.instance_ctx.clone();
                    let factory = self.factory.clone();

                    let tx = tx.clone();

                    let arb = Arbiter::new();
                    arb.spawn_fn(move || {
                        let (ctx, _) = factory(instance_id);
                        WatchdogActor::start(tx);
                        instance_ctx.write().register(ctx);
                    });
                }

                KillerActor::from_registry();
                Ok(ServerHandler::NoHTTP(ArbiterHandler::new(cpus, rx)))
            }
            ServerMode::HTTP { port } => {
                let instance_ctx = self.instance_ctx;
                let factory = self.factory;

                let srv = HttpServer::new(move || {
                    let (ctx, http_services) = factory(instance_id);
                    instance_ctx.write().register(ctx.clone());
                    App::new()
                        .wrap(Logger::default())
                        .app_data(Data::new(ctx))
                        .app_data(Data::from(Arc::new(instance_ctx.read().clone())))
                        .configure(http_services)
                })
                .bind(port)
                .unwrap()
                .run();

                KillerActor::from_registry().do_send(RegisterHttpServer::new(srv.clone()));
                Ok(ServerHandler::HTTP(srv))
            }
        }
    }
}
