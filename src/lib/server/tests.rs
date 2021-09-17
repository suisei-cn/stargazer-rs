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
    use std::thread;
    use std::time::Duration;

    use actix::{AsyncContext, System};
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::{sleep, timeout};

    use crate::KillerActor;

    #[test]
    fn must_kill_system() {
        // spawn a new thread to avoid polluting thread local storage
        thread::spawn(|| {
            let (tx, mut rx) = unbounded_channel();

            {
                let sys = System::new();
                sys.block_on(async {
                    // clone the channel so that it won't be closed
                    let tx = tx.clone();

                    actix::spawn(async move {
                        sleep(Duration::from_millis(100)).await;
                        // Killer failed to reap this system, notify main thread
                        tx.send(());
                        System::current().stop(); // cleanup
                    });

                    KillerActor::start(None);
                    KillerActor::kill(true);
                });
                sys.run(); // join system
            }

            let sys = System::new();
            sys.block_on(async {
                // Here we spawned a future and use recv instead of blocking_recv
                // because we don't want the test stuck forever if it's failed.
                let fut = timeout(Duration::from_millis(150), rx.recv());
                assert!(fut.await.is_err(), "system is not killed");
            });
        })
        .join()
        .unwrap()
    }

    #[test]
    fn must_not_create_multiple_killer() {
        // spawn a new thread to avoid polluting thread local storage
        if let Err(e) = thread::spawn(|| {
            let sys = System::new();
            sys.block_on(async {
                for _ in 0..2 {
                    KillerActor::start(None);
                }
                System::current().stop()
            });
        })
        .join()
        {
            assert_eq!(
                e.downcast_ref::<&'static str>().expect("unexpected panic"),
                &"cannot run two killers on the same arbiter"
            );
        } else {
            panic!("multiple killer is created");
        }
    }
}

mod watchdog {
    use std::mem;
    use std::thread;
    use std::time::Duration;

    use actix::System;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::timeout;

    use crate::server::watchdog::WatchdogActor;

    #[test]
    fn must_send_when_stopped() {
        // spawn a new thread to avoid polluting thread local storage
        thread::spawn(|| {
            let (tx, mut rx) = unbounded_channel();
            {
                let sys = System::new();
                sys.block_on(async {
                    WatchdogActor::start(tx.clone()); // start watchdog
                    System::current().stop(); // trigger watchdog
                });
                sys.run(); // join system
            }

            let sys = System::new();
            sys.block_on(async {
                let fut = timeout(Duration::from_millis(100), rx.recv());
                assert!(fut.await.is_ok(), "watchdog failed to send die signal");
            });
        })
        .join()
        .unwrap();
    }

    #[test]
    fn must_not_create_multiple_watchdog() {
        // spawn a new thread to avoid polluting thread local storage
        if let Err(e) = thread::spawn(|| {
            let sys = System::new();
            sys.block_on(async {
                for _ in 0..2 {
                    let (tx, rx) = unbounded_channel();
                    // leak rx so that the channel won't be closed causing unexpected panics
                    mem::forget(rx);
                    WatchdogActor::start(tx);
                }
                System::current().stop()
            });
        })
        .join()
        {
            assert_eq!(
                e.downcast_ref::<&'static str>().expect("unexpected panic"),
                &"cannot run two watchdogs on the same arbiter"
            );
        } else {
            panic!("multiple watchdog is created");
        }
    }
}
