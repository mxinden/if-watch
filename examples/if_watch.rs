use if_watch::IfWatcher;
use std::pin::Pin;

fn main() {
    env_logger::init();
    futures::executor::block_on(async {
        let mut set = IfWatcher::new().await.unwrap();
        loop {
            println!("Got event {:?}", Pin::new(&mut set).await);
        }
    });
}
