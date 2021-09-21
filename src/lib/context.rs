use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use actix::dev::ToEnvelope;
use actix::{Actor, Addr, Handler, MailboxError, Message};
use itertools::Itertools;
use once_cell::unsync::OnceCell;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::request::{MsgRequest, MsgRequestTuple, MsgRequestVec, RequestTrait};
use crate::scheduler::messages::GetId;
use crate::utils::TypeEq;

type StdResult<T, E> = std::result::Result<T, E>;

thread_local! {
    static LOCAL_ARBITER_CONTEXT: OnceCell<ArbiterContext> = OnceCell::new();
}

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
    /// Set the thread local arbiter context.
    ///
    /// # Panics
    /// Panics when the local arbiter has already been set.
    pub fn set(obj: Self) {
        LOCAL_ARBITER_CONTEXT.with(|cell| {
            cell.set(obj)
                .expect("can't set local arbiter context again");
        });
    }
    /// Try to get the thread local arbiter context.
    /// Returns `None` if no arbiter context is registered on current thread.
    pub fn try_get() -> Option<Self> {
        LOCAL_ARBITER_CONTEXT.with(|cell| cell.get().cloned())
    }
    /// Get the thread local arbiter context.
    ///
    /// # Panics
    /// Panics when no arbiter context is registered on current thread.
    pub fn get() -> Self {
        Self::try_get().expect("no arbiter context available")
    }
    /// Acquires a reference to the thread local arbiter context.
    ///
    /// # Panics
    /// Panics when no arbiter context is registered on current thread.
    #[allow(clippy::use_self)]
    pub fn with<F, R>(f: F) -> R
    where
        F: FnOnce(&ArbiterContext) -> R,
    {
        LOCAL_ARBITER_CONTEXT.with(|cell| f(cell.get().unwrap()))
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

impl InstanceContext {
    pub const fn instance_id(&self) -> Uuid {
        self.instance_id
    }
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
    pub fn send<'a, Target, Act, AddrT, MsgT, Output>(
        &'a self,
        target: Target,
        msg: &MsgT,
    ) -> Result<impl RequestTrait<Output = StdResult<HashMap<Uuid, Output>, MailboxError>> + 'a>
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
                |(ids, resps)| {
                    let ids: Vec<_> = ids.into_iter().try_collect()?;
                    let resps: Vec<_> = resps.into_iter().try_collect()?;
                    Ok(ids.into_iter().zip(resps).collect())
                },
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use actix::Actor;
    use uuid::Uuid;

    use crate::tests::{
        Adder, AdderTarget, Echo, Echo2, Echo2Target, EchoTarget, Ping, Ping2, Val,
    };
    use crate::{ArbiterContext, InstanceContext};

    #[actix::test]
    async fn must_arbiter_send() {
        let ctx = ArbiterContext::new(Uuid::new_v4()).register_addr(Echo::default().start());
        assert_eq!(
            ctx.send(EchoTarget, Ping(41, false))
                .expect("unable to find addr")
                .await
                .expect("unable to send message"),
            41
        );

        assert!(
            ctx.send(Echo2Target, Ping2(42, false)).is_err(),
            "unexpected addr"
        );
        let ctx = ctx.register_addr(Echo2::default().start());
        assert_eq!(
            ctx.send(Echo2Target, Ping2(42, false))
                .expect("unable to find addr")
                .await
                .expect("unable to send message"),
            42
        );
    }

    #[actix::test]
    async fn must_instance_send() {
        let mut inst = InstanceContext::new();
        let arb_1 = ArbiterContext::new(inst.instance_id()).register_addr(Echo::default().start());
        let arb_2 = ArbiterContext::new(inst.instance_id()).register_addr(Echo2::default().start());
        inst.register(arb_1);
        inst.register(arb_2);
        assert!(
            inst.send(EchoTarget, &Ping(41, false)).is_err(),
            "unexpected addr"
        );

        let mut inst = InstanceContext::new();
        let arb_1 = ArbiterContext::new(inst.instance_id())
            .register_addr(Echo::default().start())
            .register_addr(Adder::new(0).start());
        let arb_2 = ArbiterContext::new(inst.instance_id())
            .register_addr(Echo::default().start())
            .register_addr(Adder::new(1).start());
        inst.register(arb_1);
        inst.register(arb_2);
        let ans = inst
            .send(AdderTarget, &Val(41))
            .expect("unable to find addr")
            .await
            .expect("unable to send message");
        assert_eq!(ans.len(), 2, "length mismatch");
        assert_eq!(*ans.get(&Uuid::from_u128(0)).expect("missing key"), 41);
        assert_eq!(*ans.get(&Uuid::from_u128(1)).expect("missing key"), 42);
    }
}
