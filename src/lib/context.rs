use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use actix::dev::ToEnvelope;
use actix::{Actor, Addr, Handler, MailboxError, Message};
use derive_new::new;
use getset::CopyGetters;
use itertools::Itertools;
use once_cell::unsync::OnceCell;
use parking_lot::RwLock;
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

#[derive(Debug, Clone, new, CopyGetters)]
pub struct ArbiterContext {
    #[getset(get_copy = "pub")]
    instance_id: Uuid,
    #[new(value = "Uuid::new_v4()")]
    #[getset(get_copy = "pub")]
    arbiter_id: Uuid,
    #[new(default)]
    addrs: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
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
    pub fn send<A, M>(&self, msg: M) -> Result<MsgRequest<A, M>>
    where
        A: Actor + Handler<M>,
        A::Context: ToEnvelope<A, M>,
        M: Message + Send + 'static,
        M::Result: Send,
    {
        let addr: &Addr<A> = self
            .addrs
            .get(&TypeId::of::<Addr<A>>())
            .ok_or(Error::Context)?
            .downcast_ref()
            .unwrap();
        Ok(MsgRequest::new(addr.clone(), msg))
    }
}

#[derive(Debug, new, CopyGetters)]
pub struct InstanceContext {
    #[new(value = "Uuid::new_v4()")]
    #[getset(get_copy = "pub")]
    id: Uuid,
    #[new(value = "RwLock::new(vec![])")]
    arbiters: RwLock<Vec<ArbiterContext>>,
}

impl InstanceContext {
    pub fn register(&self, ctx: ArbiterContext) {
        self.arbiters.write().push(ctx);
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
    pub fn send<A, M>(
        &self,
        msg: &M,
    ) -> Result<impl RequestTrait<Output = StdResult<HashMap<Uuid, M::Result>, MailboxError>>>
    where
        A: Actor + Handler<M> + Handler<GetId>,
        A::Context: ToEnvelope<A, M> + ToEnvelope<A, GetId>,
        M: Message + Clone + Send + Unpin + 'static,
        M::Result: Send,
    {
        let arbiters = self.arbiters.read().clone();
        let ids: Vec<_> = arbiters
            .iter()
            .map(|arbiter| arbiter.send::<A, _>(GetId))
            .try_collect()?;
        let resps: Vec<_> = arbiters
            .iter()
            .map(|arbiter| arbiter.send::<A, _>(msg.clone()))
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

    use crate::tests::{Adder, Echo, Echo2, Ping, Ping2, Val};
    use crate::{ArbiterContext, InstanceContext};

    #[actix::test]
    async fn must_arbiter_send() {
        let ctx = ArbiterContext::new(Uuid::new_v4()).register_addr(Echo::default().start());
        assert_eq!(
            ctx.send::<Echo, _>(Ping(41, false))
                .expect("unable to find addr")
                .await
                .expect("unable to send message"),
            41
        );

        assert!(
            ctx.send::<Echo2, _>(Ping2(42, false)).is_err(),
            "unexpected addr"
        );
        let ctx = ctx.register_addr(Echo2::default().start());
        assert_eq!(
            ctx.send::<Echo2, _>(Ping2(42, false))
                .expect("unable to find addr")
                .await
                .expect("unable to send message"),
            42
        );
    }

    #[actix::test]
    async fn must_instance_send() {
        let inst = InstanceContext::new();
        let arb_1 = ArbiterContext::new(inst.id()).register_addr(Echo::default().start());
        let arb_2 = ArbiterContext::new(inst.id()).register_addr(Echo2::default().start());
        inst.register(arb_1);
        inst.register(arb_2);
        assert!(
            inst.send::<Echo, _>(&Ping(41, false)).is_err(),
            "unexpected addr"
        );

        let inst = InstanceContext::new();
        let arb_1 = ArbiterContext::new(inst.id())
            .register_addr(Echo::default().start())
            .register_addr(Adder::new(0).start());
        let arb_2 = ArbiterContext::new(inst.id())
            .register_addr(Echo::default().start())
            .register_addr(Adder::new(1).start());
        inst.register(arb_1);
        inst.register(arb_2);
        let ans = inst
            .send::<Adder, _>(&Val(41))
            .expect("unable to find addr")
            .await
            .expect("unable to send message");
        assert_eq!(ans.len(), 2, "length mismatch");
        assert_eq!(*ans.get(&Uuid::from_u128(0)).expect("missing key"), 41);
        assert_eq!(*ans.get(&Uuid::from_u128(1)).expect("missing key"), 42);
    }
}
