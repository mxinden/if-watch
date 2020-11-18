use if_watch::IfWatcher;

fn main() {
    env_logger::init();
    futures_lite::future::block_on(async {
        let mut set = IfWatcher::new().await.unwrap();
        loop {
            println!("Got event {:?}", set.next().await);
        }
    });
}
