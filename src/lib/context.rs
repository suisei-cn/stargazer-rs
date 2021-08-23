use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use actix::dev::ToEnvelope;
use actix::{Actor, Addr, Handler, Message};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::utils::TypeEq;
use crate::worker::messages::GetId;

pub trait MessageTarget: Copy {
    type Actor: Actor;
    type Addr: TypeEq<Other = Addr<Self::Actor>>;
}

#[derive(Debug, Clone)]
pub struct ArbiterContext {
    instance_id: Uuid,
    arbiter_id: Uuid,
    addrs: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl ArbiterContext {
    pub fn new(instance_id: Uuid) -> Self {
        Self {
            instance_id,
            arbiter_id: Uuid::new_v4(),
            addrs: HashMap::new(),
        }
    }

    pub fn register_addr<A: 'static + Send + Sync>(mut self, addr: A) -> Self {
        self.addrs.insert(addr.type_id(), Arc::new(addr));
        self
    }

    pub async fn send<Target, Act, AddrT, MsgT, Output>(
        &self,
        _target: Target,
        msg: MsgT,
    ) -> Result<Output>
    where
        Target: MessageTarget<Actor = Act, Addr = AddrT>,
        Act: Actor + Handler<MsgT>,
        Act::Context: ToEnvelope<Act, MsgT>,
        MsgT: Message<Result = Output> + Send + 'static,
        AddrT: 'static,
        Output: Send,
    {
        let addr: &Addr<Act> = self
            .addrs
            .get(&TypeId::of::<AddrT>())
            .ok_or(Error::Context)?
            .downcast_ref()
            .unwrap();
        Ok(addr.send(msg).await.unwrap())
    }
}

#[derive(Debug, Clone)]
pub struct InstanceContext {
    instance_id: Uuid,
    arbiters: Vec<ArbiterContext>,
}

impl Default for InstanceContext {
    fn default() -> Self {
        Self {
            instance_id: Uuid::new_v4(),
            arbiters: vec![],
        }
    }
}

impl InstanceContext {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn id(&self) -> Uuid {
        self.instance_id
    }
    pub fn register(&mut self, ctx: ArbiterContext) {
        self.arbiters.push(ctx);
    }

    pub async fn send<Target, Act, AddrT, MsgT, Output>(
        &self,
        target: Target,
        msg: MsgT,
    ) -> Result<Vec<(Uuid, Output)>>
    where
        Target: MessageTarget<Actor = Act, Addr = AddrT>,
        Act: Actor + Handler<MsgT> + Handler<GetId>,
        Act::Context: ToEnvelope<Act, MsgT>,
        Act::Context: ToEnvelope<Act, GetId>,
        MsgT: Message<Result = Output> + Clone + Send + 'static,
        AddrT: 'static,
        Output: Send,
    {
        futures::future::join_all(
            self.arbiters
                .iter()
                .map(|arbiter| (arbiter, msg.clone()))
                .map(|(arbiter, msg)| async move {
                    (
                        arbiter.send(target, GetId).await,
                        arbiter.send(target, msg).await,
                    )
                }),
        )
        .await
        .into_iter()
        .map(|(id, output)| match (id, output) {
            (Ok(id), Ok(output)) => Ok((id, output)),
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
        })
        .collect()
    }
}