use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() {
    let (sender, receiver) = mpsc::channel::<Message>();
    let (disconnect_sender, disconnect_receiver) = mpsc::channel::<Disconnect>();

    let mut reporter = CargoMsrvReporter::new(sender, disconnect_receiver);
    let _writer = CargoMsrvWriter::new(receiver, disconnect_sender);

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
    type Err;

    fn report_event(&mut self, event: Self::Event) -> Result<(), Self::Err>;

    fn disconnect(self) -> Disconnect;
}

enum Message {
    Event(Event),
    CurrentStatus(String),
    Progression(Progression),
}

enum Event {
    Installing,
    Updating(String),
}

struct Progression {
    max: u64,
    current: u64,
}

struct Disconnect;

struct CargoMsrvReporter {
    sender: mpsc::Sender<Message>,
    disconnect_receiver: mpsc::Receiver<Disconnect>,
}

impl CargoMsrvReporter {
    fn new(sender: mpsc::Sender<Message>, disconnect_receiver: mpsc::Receiver<Disconnect>) -> Self {
        Self {
            sender,
            disconnect_receiver,
        }
    }
}

impl Reporter for CargoMsrvReporter {
    type Event = Message;
    type Err = ();

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

impl CargoMsrvWriter {
    fn new(receiver: mpsc::Receiver<Message>, disconnect_sender: mpsc::Sender<Disconnect>) -> Self {
        let handle = thread::spawn(move || {
            let disconnect_sender = disconnect_sender;

            let bar = indicatif::ProgressBar::new(10);
            bar.enable_steady_tick(250);

            loop {
                match receiver.recv() {
                    Ok(message) => match message {
                        Message::Event(e) => {
                            thread::sleep(Duration::from_secs(2));
                            bar.set_message(format!("Event ({})", bar.position()))
                        }
                        Message::CurrentStatus(s) => bar.println(s),
                        Message::Progression(p) => {
                            bar.set_length(p.max);
                            bar.set_position(p.current);
                        }
                    },
                    Err(_e) => {
                        bar.finish();
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
