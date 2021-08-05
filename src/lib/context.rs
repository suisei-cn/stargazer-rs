use actix::dev::ToEnvelope;
use actix::{Actor, Addr, Handler, Message};
use frunk_core::hlist::{HList, Selector};
use uuid::Uuid;

use crate::worker::messages::GetId;

pub trait MessageTarget: Copy {
    type Actor;
}

#[derive(Debug, Clone)]
pub struct ArbiterContext<L: HList> {
    instance_id: Uuid,
    arbiter_id: Uuid,
    actors: L,
}

impl<L: HList> ArbiterContext<L> {
    pub fn new(instance_id: Uuid, actors: L) -> Self {
        Self {
            instance_id,
            arbiter_id: Uuid::new_v4(),
            actors,
        }
    }
    pub async fn send<T, A, M, O, I>(&self, _target: T, msg: M) -> O
    where
        T: MessageTarget<Actor = A>,
        A: Actor + Handler<M>,
        A::Context: ToEnvelope<A, M>,
        M: Message<Result = O> + Send + 'static,
        O: Send,
        L: Selector<Addr<A>, I>,
    {
        let actor = self.actors.get();
        actor.send(msg).await.unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct InstanceContext<L: HList> {
    instance_id: Uuid,
    arbiters: Vec<ArbiterContext<L>>,
}

impl<L: HList> Default for InstanceContext<L> {
    fn default() -> Self {
        Self {
            instance_id: Uuid::new_v4(),
            arbiters: vec![],
        }
    }
}

impl<L: HList> InstanceContext<L> {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn id(&self) -> Uuid {
        self.instance_id
    }
    pub fn register(&mut self, ctx: ArbiterContext<L>) {
        self.arbiters.push(ctx);
    }
    pub async fn send<T, A, M, O, I>(&self, target: T, msg: M) -> Vec<(Uuid, O)>
    where
        T: MessageTarget<Actor = A>,
        A: Actor + Handler<M> + Handler<GetId>,
        A::Context: ToEnvelope<A, M>,
        A::Context: ToEnvelope<A, GetId>,
        M: Message<Result = O> + Clone + Send + 'static,
        O: Send,
        L: Selector<Addr<A>, I>,
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
    }
}
