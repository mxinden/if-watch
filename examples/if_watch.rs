use futures::StreamExt;
use if_watch::IfWatcher;

fn main() {
    env_logger::init();
    futures::executor::block_on(async {
        let mut set = IfWatcher::new().unwrap();
        loop {
            let event = set.select_next_some().await;
            println!("Got event {:?}", event);
        }
    });
}
