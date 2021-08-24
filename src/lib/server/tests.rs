mod arb_handler {
    use std::time::Duration;

    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::timeout;

    use crate::server::handler::ArbiterHandler;

    #[actix::test]
    async fn must_resolve_on_all_stopped() {
        let (tx, rx) = unbounded_channel();
        let handler = ArbiterHandler::new(3, rx);

        for _ in 0..3 {
            tx.send(()).unwrap();
        }

        let fut = timeout(Duration::from_millis(100), handler);
        assert!(
            fut.await.is_ok(),
            "handler fail to resolve on all arb stopped"
        );
    }

    #[actix::test]
    async fn must_resolve_on_channel_closed() {
        let (tx, rx) = unbounded_channel();
        let handler = ArbiterHandler::new(3, rx);

        drop(tx);

        let fut = timeout(Duration::from_millis(100), handler);
        assert!(
            fut.await.is_ok(),
            "handler fail to resolve on channel closed"
        );
    }

    #[actix::test]
    async fn must_wait_on_some_running() {
        let (tx, rx) = unbounded_channel();
        let handler = ArbiterHandler::new(3, rx);

        for _ in 0..2 {
            tx.send(()).unwrap();
        }

        let fut = timeout(Duration::from_millis(100), handler);
        assert!(
            fut.await.is_err(),
            "handler unexpectedly resolved when there's still arb alive"
        );
    }
}