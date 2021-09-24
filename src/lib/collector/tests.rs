use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use futures::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, ExchangeDeclareOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::{Channel, Connection, ConnectionProperties, ExchangeKind};
use serde::{Deserialize, Serialize};
use testcontainers::clients::Cli;
use testcontainers::images::generic::{GenericImage, WaitFor};
use testcontainers::{Docker, RunArgs};
use tokio::sync::{mpsc, oneshot};
use tokio_amqp::LapinTokioExt;
use tracing_test::traced_test;

use crate::collector::amqp::AMQPFactory;
use crate::collector::debug::DebugCollectorFactory;
use crate::collector::{CollectorFactory, Publish};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct TestMsg {
    a: usize,
    b: String,
}

fn rmq_image() -> GenericImage {
    GenericImage::new("rabbitmq:3")
        .with_env_var("RABBITMQ_NODE_PORT", "5673")
        .with_wait_for(WaitFor::message_on_stdout("Server startup complete"))
}

#[actix::test]
#[traced_test]
async fn must_debug_collector() {
    let factory = DebugCollectorFactory;
    let collector = factory.build().await.expect("unable to build collector");

    let msg = TestMsg {
        a: 1,
        b: String::from("test"),
    };
    assert!(
        collector
            .send(Publish::new(String::from("blabla"), msg.clone()))
            .await
            .expect("mailbox error"),
        "unable to publish event"
    );

    assert!(logs_contain("[blabla]"), "no topic found");
    assert!(logs_contain(serde_json::to_string(&msg).unwrap().as_str()))
}

#[actix::test]
async fn must_amqp_collector() {
    if option_env!("TEST_FAST").is_some() {
        return;
    }

    if let Some(uri) = option_env!("TEST_AMQP_URI") {
        must_amqp_publish(uri).await;
        must_amqp_publish(uri).await; // reuse connection
    } else {
        let client = Cli::default();
        let _container = client.run_with_args(
            rmq_image(),
            RunArgs::default()
                .with_name("rmq")
                .with_mapped_port((5673, 5673)),
        );

        must_amqp_publish("amqp://127.0.0.1:5673").await;
        must_amqp_publish("amqp://127.0.0.1:5673").await; // reuse connection
    }
}

async fn prepare_amqp_chan(conn: &Connection) -> Channel {
    let chan = conn
        .create_channel()
        .await
        .expect("unable to create channel");
    chan.exchange_declare(
        "test",
        ExchangeKind::Topic,
        ExchangeDeclareOptions {
            durable: true,
            ..Default::default()
        },
        Default::default(),
    )
    .await
    .expect("unable to declare exchange");
    chan.queue_declare(
        "test_queue",
        QueueDeclareOptions {
            exclusive: true,
            ..Default::default()
        },
        Default::default(),
    )
    .await
    .expect("unable to create queue");
    chan.queue_bind(
        "test_queue",
        "test",
        "#",
        QueueBindOptions::default(),
        Default::default(),
    )
    .await
    .expect("unable to bind queue to exchange");
    chan
}

async fn must_amqp_publish(uri: &'static str) {
    let factory = AMQPFactory::new(uri, "test");
    let collector = factory.build().await.expect("unable to build collector");

    let received_msgs: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel();
    let (prepare_tx, prepare_rx) = oneshot::channel();

    let handler = {
        let received_msgs = received_msgs.clone();
        actix::spawn(async move {
            let conn = Connection::connect(uri, ConnectionProperties::default().with_tokio())
                .await
                .expect("unable to connect to rabbitmq");
            let chan = prepare_amqp_chan(&conn).await;

            let mut consumer = chan
                .basic_consume(
                    "test_queue",
                    "test_consumer",
                    BasicConsumeOptions::default(),
                    Default::default(),
                )
                .await
                .expect("unable to create consume");
            prepare_tx.send(()).unwrap();
            loop {
                tokio::select! {
                    msg = consumer.next() => {
                        let (chan, delivery) = msg.unwrap().expect("error in consumer");
                        received_msgs.borrow_mut().push(delivery.data);
                        chan.basic_ack(delivery.delivery_tag, BasicAckOptions::default())
                            .await
                            .expect("ack fail");
                    },
                    _ = stop_rx.recv() => break
                }
            }
        })
    };

    prepare_rx.await.unwrap();

    let msg = TestMsg {
        a: 1,
        b: String::from("test"),
    };
    assert!(
        collector
            .send(Publish::new(String::from("blabla"), msg.clone()))
            .await
            .expect("mailbox error"),
        "unable to publish event"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    stop_tx.send(()).unwrap();
    let received_msgs_ref = received_msgs.borrow();
    assert_eq!(received_msgs_ref.len(), 1);
    let received_msg: TestMsg = serde_json::from_slice(&*received_msgs_ref.first().unwrap())
        .expect("unable to deserialize msg");
    assert_eq!(msg, received_msg);

    handler.await.unwrap();
}
