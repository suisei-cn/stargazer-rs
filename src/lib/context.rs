use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use actix::dev::ToEnvelope;
use actix::{Actor, Addr, Handler, MailboxError, Message};
use itertools::Itertools;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::request::{MsgRequest, MsgRequestTuple, MsgRequestVec, RequestTrait};
use crate::utils::TypeEq;
use crate::worker::messages::GetId;

type StdResult<T, E> = std::result::Result<T, E>;

pub trait MessageTarget: Copy + Send {
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
    pub const fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    pub const fn arbiter_id(&self) -> Uuid {
        self.arbiter_id
    }
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

    /// Send a message to a registered actor.
    ///
    /// # Panics
    ///
    /// Panics when `MessageTarget` invariant doesn't hold (The address and actor type doesn't match).
    /// This should never happen due to the `TypeEq` bound.
    ///
    /// # Errors
    ///
    /// Raise an [`Error::Context`](Error::Context) when there's no such actor in the context.
    pub fn send<Target, Act, AddrT, MsgT>(
        &self,
        _target: Target,
        msg: MsgT,
    ) -> Result<MsgRequest<'_, Act, MsgT>>
    where
        Target: MessageTarget<Actor = Act, Addr = AddrT>,
        Act: Actor + Handler<MsgT>,
        Act::Context: ToEnvelope<Act, MsgT>,
        MsgT: Message + Send + 'static,
        MsgT::Result: Send,
        AddrT: 'static,
    {
        let addr: &Addr<Act> = self
            .addrs
            .get(&TypeId::of::<AddrT>())
            .ok_or(Error::Context)?
            .downcast_ref()
            .unwrap();
        Ok(MsgRequest::new(addr, msg))
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
    pub const fn id(&self) -> Uuid {
        self.instance_id
    }
    pub fn register(&mut self, ctx: ArbiterContext) {
        self.arbiters.push(ctx);
    }

    /// Send a message to a registered actor.
    ///
    /// # Panics
    ///
    /// Panics when `MessageTarget` invariant doesn't hold (The address and actor type doesn't match).
    /// This should never happen due to the `TypeEq` bound.
    ///
    /// # Errors
    ///
    /// Raise an [`Error::Context`](Error::Context) when there's no such actor in the context.
    pub fn send<'a, Target, Act, AddrT, MsgT, Output, R>(
        &'a self,
        target: Target,
        msg: &MsgT,
    ) -> Result<impl RequestTrait + 'a>
    where
        Target: MessageTarget<Actor = Act, Addr = AddrT>,
        Act: Actor + Handler<MsgT> + Handler<GetId>,
        Act::Context: ToEnvelope<Act, MsgT> + ToEnvelope<Act, GetId>,
        MsgT: Message<Result = Output> + Clone + Send + Unpin + 'static,
        AddrT: 'static,
        Output: Send + 'a,
    {
        // MsgRequestVec::new(
        let ids: Vec<_> = self
            .arbiters
            .iter()
            .map(|arbiter| arbiter.send(target, GetId))
            .try_collect()?;
        let resps: Vec<_> = self
            .arbiters
            .iter()
            .map(|arbiter| arbiter.send(target, msg.clone()))
            .try_collect()?;
        Ok(
            MsgRequestTuple::new(MsgRequestVec::new(ids), MsgRequestVec::new(resps)).map(
                |(ids, resps)| -> StdResult<_, MailboxError> {
                    let ids: Vec<_> = ids.into_iter().try_collect()?;
                    let resps: Vec<_> = resps.into_iter().try_collect()?;
                    Ok(ids.into_iter().zip(resps).collect_vec())
                },
            ),
        )
    }
}
