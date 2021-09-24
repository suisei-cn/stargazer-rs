use actix::{Actor, Context, Handler, Recipient};
use async_trait::async_trait;
use tracing::{info, info_span};

use crate::collector::{Collector, CollectorFactory, Publish};

#[derive(Debug)]
pub struct DebugCollectorFactory;

#[async_trait]
impl CollectorFactory for DebugCollectorFactory {
    fn ident(&self) -> String {
        String::from("debug")
    }

    async fn build(&self) -> Option<Recipient<Publish>> {
        let addr = DebugCollector.start();
        Some(addr.recipient())
    }
}

#[derive(Debug)]
pub struct DebugCollector;

impl Actor for DebugCollector {
    type Context = Context<Self>;
}

impl Handler<Publish> for DebugCollector {
    type Result = bool;

    fn handle(&mut self, msg: Publish, _ctx: &mut Self::Context) -> Self::Result {
        let output = serde_json::to_string(&*msg.data).unwrap();
        info_span!("debug").in_scope(|| info!("collected: [{}] {}", msg.topic, output));
        true
    }
}

impl Collector for DebugCollector {}
