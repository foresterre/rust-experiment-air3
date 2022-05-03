#![allow(unused)]

use std::io::{Stderr, Stdout, StdoutLock, Write};
use std::marker::PhantomData;
use std::process::Stdio;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use std::{io, thread};

fn main() {
    let (sender, receiver) = mpsc::channel::<Message>();
    let (disconnect_sender, disconnect_receiver) = mpsc::channel::<Disconnect>();

    let mut reporter = CargoMsrvReporter::setup(sender, disconnect_receiver);

    let indicatif_handler = IndicatifHandler::default();
    let json_handler = JsonHandler::default();
    let multi_handler = MultiHandler::new()
        .push(Box::new(json_handler))
        .push(Box::new(indicatif_handler));

    let _writer = CargoMsrvWriter::setup(receiver, disconnect_sender, multi_handler);

    reporter.report_event(Message::CurrentStatus("chris!".into()));
    reporter.report_event(Message::CurrentStatus("jean!".into()));
    reporter.report_event(Message::Progression(Progression {
        current: 1,
        max: 10,
    }));
    reporter.report_event(Message::CurrentStatus("chris!".into()));

    reporter.report_event(Message::Event(Event::Installing));

    reporter.report_event(Message::Progression(Progression {
        current: 5,
        max: 10,
    }));
    reporter.report_event(Message::Event(Event::Installing));

    reporter.report_event(Message::Progression(Progression {
        current: 10,
        max: 10,
    }));

    reporter.report_event(Message::Event(Event::Installing));

    let _ = reporter.disconnect();
}

trait Reporter {
    type Event;
    type Disconnect;
    type Err;

    fn setup(
        sender: mpsc::Sender<Self::Event>,
        disconnect_receiver: mpsc::Receiver<Self::Disconnect>,
    ) -> Self;

    fn report_event(&mut self, event: Self::Event) -> Result<(), Self::Err>;

    fn disconnect(self) -> Disconnect;
}

#[derive(serde::Serialize, Clone)]
enum Message {
    Event(Event),
    CurrentStatus(String),
    Progression(Progression),
}

#[derive(serde::Serialize, Clone)]
enum Event {
    Installing,
    Updating(String),
}

#[derive(serde::Serialize, Clone)]
struct Progression {
    max: u64,
    current: u64,
}

struct Disconnect;

struct CargoMsrvReporter {
    sender: mpsc::Sender<Message>,
    disconnect_receiver: mpsc::Receiver<Disconnect>,
}

impl Reporter for CargoMsrvReporter {
    type Event = Message;
    type Disconnect = Disconnect;
    type Err = ();

    fn setup(sender: Sender<Self::Event>, disconnect_receiver: Receiver<Self::Disconnect>) -> Self {
        Self {
            sender,
            disconnect_receiver,
        }
    }

    fn report_event(&mut self, event: Self::Event) -> Result<(), Self::Err> {
        self.sender.send(event).map_err(|_| ())
    }

    fn disconnect(self) -> Disconnect {
        drop(self.sender);

        self.disconnect_receiver.recv().unwrap()
    }
}

struct CargoMsrvWriter {
    handle: thread::JoinHandle<()>,
}

trait EventWriter {
    type Event;
    type Disconnect;

    fn setup<H>(
        receiver: mpsc::Receiver<Self::Event>,
        disconnect_sender: mpsc::Sender<Self::Disconnect>,
        handler: H,
    ) -> Self
    where
        H: EventHandler<Event = Self::Event>;
}

impl EventWriter for CargoMsrvWriter {
    type Event = Message;
    type Disconnect = Disconnect;

    fn setup<H>(
        receiver: Receiver<Self::Event>,
        disconnect_sender: Sender<Self::Disconnect>,
        handler: H,
    ) -> Self
    where
        H: EventHandler<Event = Self::Event>,
    {
        let handle = thread::spawn(move || {
            let disconnect_sender = disconnect_sender;

            loop {
                match receiver.recv() {
                    Ok(message) => handler.handle(message),
                    Err(_e) => {
                        handler.finish();
                        eprintln!("\n\nSender closed!");
                        disconnect_sender.send(Disconnect).unwrap();
                        break;
                    }
                }
            }
        });

        Self { handle }
    }
}

trait EventHandler: Send + 'static {
    type Event;

    fn handle(&self, event: Self::Event);

    fn finish(&self);
}

struct IndicatifHandler {
    bar: indicatif::ProgressBar,
}

impl Default for IndicatifHandler {
    fn default() -> Self {
        let bar = indicatif::ProgressBar::new(10);
        bar.enable_steady_tick(250);

        Self { bar }
    }
}

impl EventHandler for IndicatifHandler {
    type Event = Message;

    fn handle(&self, event: Self::Event) {
        match event {
            Message::Event(e) => {
                thread::sleep(Duration::from_secs(2));
                self.bar
                    .set_message(format!("Event ({})", self.bar.position()))
            }
            Message::CurrentStatus(s) => self.bar.println(s),
            Message::Progression(p) => {
                self.bar.set_length(p.max);
                self.bar.set_position(p.current);
            }
        }
    }

    fn finish(&self) {
        self.bar.finish();
    }
}

struct JsonHandler {
    stdout: Arc<Mutex<Stderr>>,
}

impl Default for JsonHandler {
    fn default() -> Self {
        Self {
            stdout: Arc::new(Mutex::new(io::stderr())),
        }
    }
}

impl EventHandler for JsonHandler {
    type Event = Message;

    fn handle(&self, event: Self::Event) {
        let message = serde_json::to_string(&event).unwrap_or_default();

        let mut out = self.stdout.lock().unwrap();
        write!(out, "{}\n", message);
        out.flush();
    }

    fn finish(&self) {}
}

struct MultiHandler {
    handlers: Vec<Box<dyn EventHandler<Event = Message>>>,
}

impl MultiHandler {
    fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    fn push(mut self, handler: Box<dyn EventHandler<Event = Message>>) -> Self {
        self.handlers.push(handler);
        self
    }
}

impl EventHandler for MultiHandler {
    type Event = Message;

    fn handle(&self, event: Self::Event) {
        for handler in &self.handlers {
            handler.handle(event.clone())
        }
    }

    fn finish(&self) {
        for handler in &self.handlers {
            handler.finish()
        }
    }
}
