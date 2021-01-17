#[macro_use]
extern crate serde_derive;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use getopts::Options;
use rss::{ChannelBuilder, Item, ItemBuilder};
// use serde::{Deserialize, Serialize};
use nix::unistd::{chown, Uid};
use url::Url;
use users::get_user_by_name;
use walkdir::{DirEntry, WalkDir};

// const NGINX_STATIC_DIR: &'static str = "/var/www/html/aircheq-podcast/";
const TARGET_EXTS: [&'static str; 5] = ["m4a", "aac", "mp4", "flv", "m2ts"];
const DEFAULT_CONFIG_PATH: &'static str = "/etc/aircheq-podcast/config.json";
const XML_FILENAME: &'static str = "feed.xml";
const DEFAULT_ROOT_URL: &'static str = "http://127.0.0.1/";

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    query: Vec<String>,
}

fn generate_options() -> Options {
    let mut opts = Options::new();
    opts.optopt("i", "src", "Crawl dir", "crawl_dir");
    opts.optopt("o", "dst", "NGINX root dir", "nginx_static_dir");
    opts.optopt("u", "url", "URL root", "url_root");
    opts.optopt("c", "config", "Config path", "config_path");
    opts
}
fn make_default_config<P: AsRef<Path>>(path: P) -> anyhow::Result<()> {
    let jsonfile = BufWriter::new(fs::File::create(&path)?);

    let config = Config {
        query: vec![
            "オードリー",
            "深夜の馬鹿力",
            "カーボーイ",
            "佐久間宣行",
            "ハライチ",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    };
    serde_json::to_writer_pretty(jsonfile, &config)?;

    Ok(())
}

fn read_config<P: AsRef<Path>>(path: P) -> anyhow::Result<Config> {
    let config_file = fs::File::open(path)?;
    let reader = BufReader::new(config_file);
    let config = serde_json::from_reader(reader)?;
    Ok(config)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let matches = generate_options().parse(&args[1..])?;
    let crawl_dir = PathBuf::from(matches.opt_str("i").expect("Argument required: crawl_dir"));
    let nginx_static_dir = matches
        .opt_str("o")
        .expect("Argument required: nginx_static_dir");
    let url_root: Url =
        Url::parse(&(matches.opt_str("u").unwrap_or(DEFAULT_ROOT_URL.to_string())))?;
    let config_path = PathBuf::from(
        shellexpand::tilde(
            &(matches
                .opt_str("c")
                .unwrap_or(DEFAULT_CONFIG_PATH.to_string())),
        )
        .to_string(),
    );

    if !&config_path.exists() {
        fs::create_dir_all(&config_path.parent().unwrap())?;
        make_default_config(&config_path)?;
    }
    let config = read_config(&config_path)?;

    let root_dir = PathBuf::from(nginx_static_dir);
    let dir_items: Vec<DirEntry> = config
        .query
        .iter()
        .map(|q| {
            let ts = WalkDir::new(&crawl_dir).into_iter().filter(|e| {
                let entry = e.as_ref().unwrap();

                // filter with file ext
                let ext = entry.path().extension().unwrap_or(std::ffi::OsStr::new(""));
                let target_ext = TARGET_EXTS.contains(&ext.to_str().unwrap());

                // filter name with query
                let matched = entry
                    .file_name()
                    .to_str()
                    .map(|filename| filename.contains(q))
                    .unwrap_or(false);

                matched && target_ext
            });
            // choose latest with timestamp
            ts.into_iter()
                .max_by_key(|content| {
                    let c = content.as_ref().unwrap();

                    let timestamp = c
                        .metadata()
                        .map(|metadata| metadata.created())
                        .unwrap()
                        .unwrap();

                    timestamp
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map(|created| created.as_secs())
                        .unwrap();
                })
                .unwrap()
                .unwrap()
        })
        .collect();

    let items: Vec<Item> = dir_items
        .iter()
        .map(|target| {
            let src = &target.path();
            let ext = &src.extension().unwrap_or(OsStr::new("")).to_str().unwrap();

            let filename = &target.file_name().to_str().unwrap();
            let mut dst: PathBuf = root_dir.join(&filename);

            let published = if ext == &"m2ts" {
                dst.set_extension("mp4");
                let ffmpeg_cmd = format!(
                    "ffmpeg -y -i \"{src}\" -c:v copy -c:a copy \"{dst}\"",
                    src = &src.to_str().unwrap(),
                    dst = &dst.to_str().unwrap()
                );

                subprocess::Exec::shell(ffmpeg_cmd)
                    .join()
                    .expect("failed to execute FFMpeg");
                dst
            } else {
                fs::copy(&src, &dst).unwrap();
                dst
            };
            let nginx_user = get_user_by_name("nginx").expect("user not found.");
            let uid = Uid::from_raw(nginx_user.uid());
            chown(&published, Some(uid), None).expect("permission not changed");
            let filename = &published
                .file_name()
                .unwrap_or(OsStr::new(""))
                .to_str()
                .unwrap();
            ItemBuilder::default()
                .title(filename.to_string())
                .link(
                    url_root
                        .join(&filename)
                        .expect("urljoin failed")
                        .into_string(),
                )
                .build()
                .expect("failed to build an item of feed")
        })
        .collect();

    let channel = ChannelBuilder::default()
        .title("aircheq-podcast")
        .description("aircheq podcast server")
        .items(items)
        .build()
        .expect("failed to build a feed.");

    let feed_path = root_dir.join(XML_FILENAME);
    let feed = BufWriter::new(fs::File::create(feed_path)?);
    channel.pretty_write_to(feed, b' ', 4).unwrap();
    Ok(())
}
