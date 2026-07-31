//! `kompo_tree` against the `kompo_storage` trie on the same filesystem image.
//!
//! Both sides are measured the way `kompo_fs::glue` actually calls them: the
//! trie gets an absolute path split into components (what `glue` builds today),
//! the tree gets the same path as bytes (what it would be handed instead). The
//! split is part of the trie's cost because the trie's API requires it.

use criterion::{Criterion, criterion_group, criterion_main};
use std::ffi::OsStr;
use std::hint::black_box;
use std::path::Path;

static SMALL: &[u8] = &[b'#'; 512];
static MEDIUM: &[u8] = &[b'#'; 4096];
static LARGE: &[u8] = &[b'#'; 32768];

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// A Rails app with its bundled gems, emitted gem by gem like the real image.
fn image() -> Vec<(&'static str, &'static [u8])> {
    let mut out: Vec<(&'static str, &'static [u8])> = Vec::new();

    for dir in [
        "models",
        "controllers",
        "views",
        "helpers",
        "jobs",
        "mailers",
        "channels",
    ] {
        for i in 0..30 {
            out.push((
                leak(format!(
                    "/app/{}/{}{}.rb",
                    dir,
                    dir.trim_end_matches('s'),
                    i
                )),
                MEDIUM,
            ));
        }
    }
    for version in ["v1", "v2"] {
        for i in 0..20 {
            out.push((
                leak(format!(
                    "/app/controllers/api/{}/controller{}.rb",
                    version, i
                )),
                MEDIUM,
            ));
        }
    }
    for file in ["application.rb", "environment.rb", "routes.rb", "puma.rb"] {
        out.push((leak(format!("/config/{}", file)), SMALL));
    }
    for i in 0..20 {
        out.push((
            leak(format!("/config/initializers/initializer{}.rb", i)),
            SMALL,
        ));
    }
    for i in 0..50 {
        out.push((leak(format!("/lib/lib{}.rb", i)), MEDIUM));
    }

    let gems = [
        "rails",
        "activerecord",
        "actionpack",
        "activesupport",
        "actionview",
        "actionmailer",
        "activejob",
        "actioncable",
        "railties",
        "bundler",
        "rake",
        "thor",
        "i18n",
        "tzinfo",
        "concurrent-ruby",
        "sprockets",
        "puma",
        "bootsnap",
        "byebug",
        "listen",
        "spring",
        "capybara",
        "rspec-rails",
        "factory_bot",
        "faker",
        "devise",
        "pundit",
        "sidekiq",
        "redis",
        "pg",
        "aws-sdk-s3",
        "carrierwave",
        "mini_magick",
        "nokogiri",
        "rack",
        "sinatra",
        "sequel",
        "oj",
        "msgpack",
        "webmock",
    ];
    for gem in gems {
        let count = match gem {
            "rails" | "activerecord" | "actionpack" | "activesupport" => 200,
            "aws-sdk-s3" => 150,
            _ => 50,
        };
        for i in 0..count {
            out.push((
                leak(format!(
                    "/vendor/bundle/ruby/3.2.0/gems/{}/lib/{}{}.rb",
                    gem,
                    gem.replace('-', "_"),
                    i
                )),
                if i < 5 { LARGE } else { MEDIUM },
            ));
        }
        if count > 100 {
            for subdir in ["core", "util", "ext"] {
                for i in 0..20 {
                    out.push((
                        leak(format!(
                            "/vendor/bundle/ruby/3.2.0/gems/{}/lib/{}/{}{}.rb",
                            gem, subdir, subdir, i
                        )),
                        MEDIUM,
                    ));
                }
            }
        }
    }

    out
}

fn build_trie(image: &[(&'static str, &'static [u8])]) -> kompo_storage::Fs<'static> {
    let mut builder: trie_rs::map::TrieBuilder<&OsStr, &[u8]> = trie_rs::map::TrieBuilder::new();
    for (path, data) in image {
        let comps: Vec<&OsStr> = Path::new(*path).iter().collect();
        builder.push(&comps, *data);
    }
    kompo_storage::Fs::new(builder)
}

/// The blobs the linker would hand us: assembled once, outside the measurement.
fn blobs(
    image: &[(&'static str, &'static [u8])],
) -> (&'static [u8], &'static [u8], &'static [u64]) {
    let mut paths = Vec::new();
    let mut files = Vec::new();
    let mut offsets = vec![0u64];
    for (path, data) in image {
        paths.extend_from_slice(path.as_bytes());
        paths.push(0);
        files.extend_from_slice(data);
        offsets.push(files.len() as u64);
    }
    (Vec::leak(paths), Vec::leak(files), Vec::leak(offsets))
}

fn build_tree(image: &[(&'static str, &'static [u8])]) -> kompo_tree::Fs<'static> {
    let (paths, files, offsets) = blobs(image);
    kompo_tree::Fs::new(paths, files, offsets)
}

/// What `glue::stat_from_fs` does before it can call the trie.
fn split(path: &str) -> Vec<&OsStr> {
    Path::new(path).iter().collect()
}

const SHALLOW: &str = "/app/models/model0.rb";
const DEEP: &str = "/vendor/bundle/ruby/3.2.0/gems/rails/lib/rails0.rb";
/// A name `require` probes and does not find, inside a directory that exists.
const MISS: &str = "/vendor/bundle/ruby/3.2.0/gems/rails/lib/rails0.so";

/// Startup cost: the work done on the first intercepted syscall.
///
/// Both sides start from data already linked into the binary, so the blobs are
/// assembled once up front and only the indexing is timed.
fn bench_build(c: &mut Criterion) {
    let image = image();
    let (paths, files, offsets) = blobs(&image);

    let mut group = c.benchmark_group("build");
    group.sample_size(10);
    group.bench_function("trie", |b| b.iter(|| build_trie(black_box(&image))));
    group.bench_function("tree", |b| {
        b.iter(|| kompo_tree::Fs::new(black_box(paths), black_box(files), black_box(offsets)))
    });
    group.finish();
}

fn bench_stat(c: &mut Criterion) {
    let image = image();
    let trie = build_trie(&image);
    let tree = build_tree(&image);

    let mut group = c.benchmark_group("stat");
    for (name, path) in [("shallow", SHALLOW), ("deep", DEEP), ("miss", MISS)] {
        group.bench_function(format!("trie/{name}"), |b| {
            b.iter(|| {
                let mut st: libc::stat = unsafe { std::mem::zeroed() };
                let comps = split(black_box(path));
                black_box(trie.stat(&comps, &mut st))
            })
        });
        group.bench_function(format!("tree/{name}"), |b| {
            b.iter(|| {
                let mut st: libc::stat = unsafe { std::mem::zeroed() };
                black_box(tree.stat(black_box(path.as_bytes()), &mut st))
            })
        });
    }
    group.finish();
}

fn bench_require(c: &mut Criterion) {
    let image = image();
    let trie = build_trie(&image);
    let tree = build_tree(&image);

    // stat -> open -> read to EOF -> close, the shape of a single `require`.
    let mut group = c.benchmark_group("require");
    group.bench_function("trie", |b| {
        b.iter(|| {
            let comps = split(black_box(DEEP));
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            trie.stat(&comps, &mut st);
            let fd = trie.open(&comps).unwrap();
            let mut buf = [0u8; 8192];
            while trie.read(fd, &mut buf).unwrap_or(0) > 0 {}
            trie.close(fd);
            unsafe { libc::close(fd) };
        })
    });
    group.bench_function("tree", |b| {
        b.iter(|| {
            let path = black_box(DEEP.as_bytes());
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            tree.stat(path, &mut st);
            let fd = tree.open(path).unwrap();
            let mut buf = [0u8; 8192];
            while tree.read(fd, &mut buf).unwrap_or(0) > 0 {}
            tree.close(fd);
            unsafe { libc::close(fd) };
        })
    });
    group.finish();
}

fn bench_readdir(c: &mut Criterion) {
    let image = image();
    let trie = build_trie(&image);
    let tree = build_tree(&image);

    let dirs = [
        ("app_models", "/app/models"),
        ("gem_lib", "/vendor/bundle/ruby/3.2.0/gems/rails/lib"),
        ("gems", "/vendor/bundle/ruby/3.2.0/gems"),
    ];

    let mut group = c.benchmark_group("readdir");
    group.sample_size(20);
    for (name, path) in dirs {
        group.bench_function(format!("trie/{name}"), |b| {
            let comps = split(path);
            b.iter(|| {
                let mut dir = trie.opendir(black_box(&comps)).unwrap();
                let mut n = 0u32;
                loop {
                    match trie.readdir(&mut dir) {
                        Some(p) if !p.is_null() => {
                            n += 1;
                            // The trie allocates a dirent per entry; `glue`
                            // never frees it, so free it here to bench the
                            // work rather than the leak.
                            unsafe { drop(Box::from_raw(p)) };
                        }
                        _ => break,
                    }
                }
                let fd = dir.fd;
                trie.closedir(&dir);
                unsafe { libc::close(fd) };
                n
            })
        });
        group.bench_function(format!("tree/{name}"), |b| {
            b.iter(|| {
                let mut dir = tree.opendir(black_box(path.as_bytes())).unwrap();
                let mut n = 0u32;
                loop {
                    match tree.readdir(&mut dir) {
                        Some(p) if !p.is_null() => n += 1,
                        _ => break,
                    }
                }
                let fd = dir.fd;
                tree.closedir(&dir);
                unsafe { libc::close(fd) };
                n
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_build,
    bench_stat,
    bench_require,
    bench_readdir
);
criterion_main!(benches);
