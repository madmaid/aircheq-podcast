use std::fs;
use std::path::Path;

use rss::ChannelBuilder;

fn main() -> Result<(), std::io::Error> {
    let crawl_pathes = vec!["/home/madmaid/recorded"];

    let path = Path::new(crawl_pathes[0]);

    for content in fs::read_dir(path)? {
        let f = content?;
        println!("{}", f.file_name().into_string().unwrap());
        println!("{}", f.metadata()?.created()?.elapsed().unwrap().as_secs());
    }
    let channel = ChannelBuilder::default()
        .title("aircheq-podcast")
        .build()
        .unwrap();
    println!("{}", channel.title());
    Ok(())

    // serde_json::from_str();
}
