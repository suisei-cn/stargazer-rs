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

mod killer {
    use std::time::Duration;

    use actix::{AsyncContext, System, SystemService};
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::{sleep, timeout};

    use crate::server::killer::Kill;
    use crate::KillerActor;

    #[test]
    fn must_kill_system() {
        let sys = System::new();
        let (tx, mut rx) = unbounded_channel();

        sys.block_on(async {
            // clone the channel so that it won't be closed
            let tx = tx.clone();

            actix::spawn(async move {
                sleep(Duration::from_millis(100)).await;
                // Killer failed to reap this system, notify main thread
                tx.send(());
                System::current().stop(); // cleanup
            });

            let addr = KillerActor::from_registry();
            addr.do_send(Kill::new(true));
        });
        sys.run();

        System::new().block_on(async {
            // Here we spawned a future and use recv instead of blocking_recv
            // because we don't want the test stuck forever if it's failed.
            let fut = timeout(Duration::from_millis(150), rx.recv());
            assert!(dbg!(fut.await).is_err(), "system is not killed");
        });
    }
}

mod watchdog {
    use std::time::Duration;

    use actix::System;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::timeout;

    use crate::server::watchdog::WatchdogActor;

    #[test]
    fn must_send_when_stopped() {
        let sys = System::new();
        let (tx, mut rx) = unbounded_channel();
        sys.block_on(async {
            WatchdogActor::start(tx.clone()); // start watchdog
            System::current().stop(); // trigger watchdog
        });
        sys.run(); // join system

        System::new().block_on(async {
            // Here we spawned a future and use recv instead of blocking_recv
            // because we don't want the test stuck forever if it's failed.
            let fut = timeout(Duration::from_millis(100), rx.recv());
            assert!(fut.await.is_ok(), "watchdog failed to send die signal");
        });
    }
}
