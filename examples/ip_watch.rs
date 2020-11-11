use ip_watch::AddrSet;

fn main() {
    futures_lite::future::block_on(async {
        let mut set = AddrSet::new().await.unwrap();
        loop {
            println!("Got event {:?}", set.next().await);
        }
    });
}
