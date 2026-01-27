use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use kompo_storage::Fs;
use std::ffi::OsStr;
use std::hint::black_box;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use trie_rs::map::TrieBuilder;

// Realistic file sizes based on actual Ruby/Rails codebases
static SMALL_CONTENT: &[u8] = &[b'#'; 512]; // Config files, small modules (~500B)
static MEDIUM_CONTENT: &[u8] = &[b'#'; 4096]; // Typical Ruby source files (~4KB)
static LARGE_CONTENT: &[u8] = &[b'#'; 32768]; // Large library files (~32KB)
static XLARGE_CONTENT: &[u8] = &[b'#'; 131072]; // Very large files (~128KB)

/// Create a realistic Rails application filesystem
/// Simulates a medium-sized Rails app with bundled gems
/// Total: ~15,000 files (typical for Rails app + dependencies)
fn create_rails_app_fs() -> Fs<'static> {
    let mut builder: TrieBuilder<&OsStr, &[u8]> = TrieBuilder::new();

    // App directory structure (typical Rails app: ~200 files)
    let app_dirs = ["models", "controllers", "views", "helpers", "jobs", "mailers", "channels"];
    for dir in app_dirs {
        for i in 0..30 {
            let file = format!("{}{}.rb", dir.trim_end_matches('s'), i);
            let file_leaked: &'static str = Box::leak(file.into_boxed_str());
            let dir_leaked: &'static str = Box::leak(dir.to_string().into_boxed_str());
            let path: Vec<&OsStr> = vec![
                OsStr::new("app"),
                OsStr::new(dir_leaked),
                OsStr::new(file_leaked),
            ];
            builder.push(&path, MEDIUM_CONTENT);
        }
    }

    // Nested controllers (API versioning)
    for version in ["v1", "v2"] {
        for i in 0..20 {
            let file = format!("controller{}.rb", i);
            let file_leaked: &'static str = Box::leak(file.into_boxed_str());
            let version_leaked: &'static str = Box::leak(version.to_string().into_boxed_str());
            let path: Vec<&OsStr> = vec![
                OsStr::new("app"),
                OsStr::new("controllers"),
                OsStr::new("api"),
                OsStr::new(version_leaked),
                OsStr::new(file_leaked),
            ];
            builder.push(&path, MEDIUM_CONTENT);
        }
    }

    // Config files (~50 files)
    let config_files = [
        "application.rb",
        "environment.rb",
        "routes.rb",
        "database.yml",
        "secrets.yml",
        "cable.yml",
        "storage.yml",
        "puma.rb",
    ];
    for file in config_files {
        let path: Vec<&OsStr> = vec![OsStr::new("config"), OsStr::new(file)];
        builder.push(&path, SMALL_CONTENT);
    }

    // Config/initializers
    for i in 0..20 {
        let file = format!("initializer{}.rb", i);
        let file_leaked: &'static str = Box::leak(file.into_boxed_str());
        let path: Vec<&OsStr> = vec![
            OsStr::new("config"),
            OsStr::new("initializers"),
            OsStr::new(file_leaked),
        ];
        builder.push(&path, SMALL_CONTENT);
    }

    // Lib directory (~100 files)
    for i in 0..50 {
        let file = format!("lib{}.rb", i);
        let file_leaked: &'static str = Box::leak(file.into_boxed_str());
        let path: Vec<&OsStr> = vec![OsStr::new("lib"), OsStr::new(file_leaked)];
        builder.push(&path, MEDIUM_CONTENT);
    }

    // Vendor/bundle gems (the bulk: ~14,000 files simulating ~200 gems)
    let popular_gems = [
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
        "sass-rails",
        "uglifier",
        "turbolinks",
        "jbuilder",
        "puma",
        "bootsnap",
        "byebug",
        "web-console",
        "listen",
        "spring",
        "capybara",
        "selenium-webdriver",
        "rspec-rails",
        "factory_bot",
        "faker",
        "devise",
        "pundit",
        "sidekiq",
        "redis",
        "pg",
        "aws-sdk-s3",
        "paperclip",
        "carrierwave",
        "mini_magick",
    ];

    for gem in popular_gems {
        let gem_leaked: &'static str = Box::leak(gem.to_string().into_boxed_str());

        // Each gem has ~50-200 files
        let file_count = match gem {
            "rails" | "activerecord" | "actionpack" | "activesupport" => 200,
            "aws-sdk-s3" => 150,
            _ => 50,
        };

        // Main lib files
        for i in 0..file_count {
            let file = format!("{}{}.rb", gem.replace('-', "_"), i);
            let file_leaked: &'static str = Box::leak(file.into_boxed_str());
            let content = if i < 5 { LARGE_CONTENT } else { MEDIUM_CONTENT };

            let path: Vec<&OsStr> = vec![
                OsStr::new("vendor"),
                OsStr::new("bundle"),
                OsStr::new("ruby"),
                OsStr::new("3.2.0"),
                OsStr::new("gems"),
                OsStr::new(gem_leaked),
                OsStr::new("lib"),
                OsStr::new(file_leaked),
            ];
            builder.push(&path, content);
        }

        // Nested lib directories (common in larger gems)
        if file_count > 100 {
            for subdir in ["core", "util", "ext"] {
                for i in 0..20 {
                    let file = format!("{}{}.rb", subdir, i);
                    let file_leaked: &'static str = Box::leak(file.into_boxed_str());
                    let subdir_leaked: &'static str = Box::leak(subdir.to_string().into_boxed_str());
                    let path: Vec<&OsStr> = vec![
                        OsStr::new("vendor"),
                        OsStr::new("bundle"),
                        OsStr::new("ruby"),
                        OsStr::new("3.2.0"),
                        OsStr::new("gems"),
                        OsStr::new(gem_leaked),
                        OsStr::new("lib"),
                        OsStr::new(subdir_leaked),
                        OsStr::new(file_leaked),
                    ];
                    builder.push(&path, MEDIUM_CONTENT);
                }
            }
        }
    }

    Fs::new(builder)
}

// ============================================================================
// Realistic scenario benchmarks
// ============================================================================

fn bench_require_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("require_simulation");

    // Simulate Ruby's require: stat -> open -> read -> close
    group.bench_function("app_model", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("app"),
            OsStr::new("models"),
            OsStr::new("model0.rb"),
        ];
        b.iter(|| {
            // stat to check existence
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf);

            // open, read, close
            let fd = fs.open(&path).unwrap();
            let mut buf = [0u8; 8192];
            while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
            fs.close(fd);
            unsafe { libc::close(fd) };
        })
    });

    group.bench_function("gem_lib_deep", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("vendor"),
            OsStr::new("bundle"),
            OsStr::new("ruby"),
            OsStr::new("3.2.0"),
            OsStr::new("gems"),
            OsStr::new("rails"),
            OsStr::new("lib"),
            OsStr::new("rails0.rb"),
        ];
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf);

            let fd = fs.open(&path).unwrap();
            let mut buf = [0u8; 8192];
            while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
            fs.close(fd);
            unsafe { libc::close(fd) };
        })
    });

    group.bench_function("gem_lib_nested", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("vendor"),
            OsStr::new("bundle"),
            OsStr::new("ruby"),
            OsStr::new("3.2.0"),
            OsStr::new("gems"),
            OsStr::new("activerecord"),
            OsStr::new("lib"),
            OsStr::new("core"),
            OsStr::new("core0.rb"),
        ];
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf);

            let fd = fs.open(&path).unwrap();
            let mut buf = [0u8; 8192];
            while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
            fs.close(fd);
            unsafe { libc::close(fd) };
        })
    });

    group.finish();
}

fn bench_dir_glob_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("dir_glob_simulation");

    // Simulate Dir.glob pattern: opendir -> readdir* -> closedir
    group.bench_function("app_models_dir", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![OsStr::new("app"), OsStr::new("models")];
        b.iter(|| {
            let mut dir = fs.opendir(black_box(&path)).unwrap();
            let mut count = 0;
            while let Some(entry) = fs.readdir(&mut dir) {
                if entry.is_null() {
                    break;
                }
                count += 1;
                unsafe { drop(Box::from_raw(entry)) };
            }
            let fd = dir.fd;
            fs.closedir(&dir);
            unsafe { libc::close(fd) };
            count
        })
    });

    group.bench_function("gem_lib_dir_large", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("vendor"),
            OsStr::new("bundle"),
            OsStr::new("ruby"),
            OsStr::new("3.2.0"),
            OsStr::new("gems"),
            OsStr::new("rails"),
            OsStr::new("lib"),
        ];
        b.iter(|| {
            let mut dir = fs.opendir(black_box(&path)).unwrap();
            let mut count = 0;
            while let Some(entry) = fs.readdir(&mut dir) {
                if entry.is_null() {
                    break;
                }
                count += 1;
                unsafe { drop(Box::from_raw(entry)) };
            }
            let fd = dir.fd;
            fs.closedir(&dir);
            unsafe { libc::close(fd) };
            count
        })
    });

    group.bench_function("gems_dir", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("vendor"),
            OsStr::new("bundle"),
            OsStr::new("ruby"),
            OsStr::new("3.2.0"),
            OsStr::new("gems"),
        ];
        b.iter(|| {
            let mut dir = fs.opendir(black_box(&path)).unwrap();
            let mut count = 0;
            while let Some(entry) = fs.readdir(&mut dir) {
                if entry.is_null() {
                    break;
                }
                count += 1;
                unsafe { drop(Box::from_raw(entry)) };
            }
            let fd = dir.fd;
            fs.closedir(&dir);
            unsafe { libc::close(fd) };
            count
        })
    });

    group.finish();
}

// ============================================================================
// File size benchmarks
// ============================================================================

fn bench_read_by_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_by_size");

    let sizes = [
        ("512B", SMALL_CONTENT),
        ("4KB", MEDIUM_CONTENT),
        ("32KB", LARGE_CONTENT),
        ("128KB", XLARGE_CONTENT),
    ];

    for (name, content) in sizes {
        group.throughput(Throughput::Bytes(content.len() as u64));
        group.bench_with_input(BenchmarkId::new("read", name), &content, |b, &content| {
            let mut builder: TrieBuilder<&OsStr, &[u8]> = TrieBuilder::new();
            let path: Vec<&OsStr> = vec![OsStr::new("test"), OsStr::new("file.rb")];
            builder.push(&path, content);
            let fs = Fs::new(builder);

            b.iter(|| {
                let fd = fs.open(&path).unwrap();
                let mut buf = [0u8; 8192];
                let mut total = 0;
                while let Some(n) = fs.read(fd, &mut buf) {
                    if n == 0 {
                        break;
                    }
                    total += n;
                }
                fs.close(fd);
                unsafe { libc::close(fd) };
                total
            })
        });
    }

    group.finish();
}

// ============================================================================
// Path depth benchmarks
// ============================================================================

fn bench_stat_by_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("stat_by_depth");

    let fs = create_rails_app_fs();

    // Depth 2: config/routes.rb
    let depth2: Vec<&OsStr> = vec![OsStr::new("config"), OsStr::new("routes.rb")];
    group.bench_function("depth_2", |b| {
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&depth2), &mut stat_buf)
        })
    });

    // Depth 3: app/models/model0.rb
    let depth3: Vec<&OsStr> = vec![
        OsStr::new("app"),
        OsStr::new("models"),
        OsStr::new("model0.rb"),
    ];
    group.bench_function("depth_3", |b| {
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&depth3), &mut stat_buf)
        })
    });

    // Depth 5: app/controllers/api/v1/controller0.rb
    let depth5: Vec<&OsStr> = vec![
        OsStr::new("app"),
        OsStr::new("controllers"),
        OsStr::new("api"),
        OsStr::new("v1"),
        OsStr::new("controller0.rb"),
    ];
    group.bench_function("depth_5", |b| {
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&depth5), &mut stat_buf)
        })
    });

    // Depth 8: vendor/bundle/ruby/3.2.0/gems/rails/lib/rails0.rb
    let depth8: Vec<&OsStr> = vec![
        OsStr::new("vendor"),
        OsStr::new("bundle"),
        OsStr::new("ruby"),
        OsStr::new("3.2.0"),
        OsStr::new("gems"),
        OsStr::new("rails"),
        OsStr::new("lib"),
        OsStr::new("rails0.rb"),
    ];
    group.bench_function("depth_8", |b| {
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&depth8), &mut stat_buf)
        })
    });

    // Depth 9: vendor/bundle/ruby/3.2.0/gems/activerecord/lib/core/core0.rb
    let depth9: Vec<&OsStr> = vec![
        OsStr::new("vendor"),
        OsStr::new("bundle"),
        OsStr::new("ruby"),
        OsStr::new("3.2.0"),
        OsStr::new("gems"),
        OsStr::new("activerecord"),
        OsStr::new("lib"),
        OsStr::new("core"),
        OsStr::new("core0.rb"),
    ];
    group.bench_function("depth_9", |b| {
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&depth9), &mut stat_buf)
        })
    });

    group.finish();
}

// ============================================================================
// Scalability benchmarks (file count)
// ============================================================================

fn create_scaled_fs(file_count: usize) -> Fs<'static> {
    let mut builder: TrieBuilder<&OsStr, &[u8]> = TrieBuilder::new();

    // Distribute files across realistic directory structure
    let files_per_gem = 50;

    for i in 0..file_count {
        let gem_idx = i / files_per_gem;
        let file_idx = i % files_per_gem;

        let gem = format!("gem{}", gem_idx);
        let file = format!("file{}.rb", file_idx);
        let gem_leaked: &'static str = Box::leak(gem.into_boxed_str());
        let file_leaked: &'static str = Box::leak(file.into_boxed_str());

        let path: Vec<&OsStr> = vec![
            OsStr::new("vendor"),
            OsStr::new("bundle"),
            OsStr::new("ruby"),
            OsStr::new("3.2.0"),
            OsStr::new("gems"),
            OsStr::new(gem_leaked),
            OsStr::new("lib"),
            OsStr::new(file_leaked),
        ];
        builder.push(&path, MEDIUM_CONTENT);
    }

    Fs::new(builder)
}

fn bench_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");

    // Test with realistic file counts
    for file_count in [1000, 5000, 10000, 20000, 50000] {
        group.bench_with_input(
            BenchmarkId::new("stat", file_count),
            &file_count,
            |b, &count| {
                let fs = create_scaled_fs(count);
                // Access a file in the middle of the filesystem
                let gem_idx = count / 100; // Middle gem
                let gem = format!("gem{}", gem_idx);
                let gem_leaked: &'static str = Box::leak(gem.into_boxed_str());

                let path: Vec<&OsStr> = vec![
                    OsStr::new("vendor"),
                    OsStr::new("bundle"),
                    OsStr::new("ruby"),
                    OsStr::new("3.2.0"),
                    OsStr::new("gems"),
                    OsStr::new(gem_leaked),
                    OsStr::new("lib"),
                    OsStr::new("file25.rb"),
                ];
                b.iter(|| {
                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                    fs.stat(black_box(&path), &mut stat_buf)
                })
            },
        );
    }

    group.finish();
}

// ============================================================================
// Basic operation benchmarks (isolated)
// ============================================================================

fn bench_basic_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("basic_ops");

    // Pure stat (no file content)
    group.bench_function("stat_only", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("app"),
            OsStr::new("models"),
            OsStr::new("model0.rb"),
        ];
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf)
        })
    });

    // Pure fstat (fd lookup)
    group.bench_function("fstat_only", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("app"),
            OsStr::new("models"),
            OsStr::new("model0.rb"),
        ];
        let fd = fs.open(&path).unwrap();
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.fstat(black_box(fd), &mut stat_buf)
        })
    });

    // stat nonexistent (early return)
    group.bench_function("stat_nonexistent", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("nonexistent"),
            OsStr::new("path"),
            OsStr::new("file.rb"),
        ];
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf)
        })
    });

    // Pure open/close cycle
    group.bench_function("open_close_only", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("app"),
            OsStr::new("models"),
            OsStr::new("model0.rb"),
        ];
        b.iter(|| {
            let fd = fs.open(black_box(&path)).unwrap();
            fs.close(fd);
            unsafe { libc::close(fd) };
        })
    });

    group.finish();
}

// ============================================================================
// Concurrent access benchmarks (simulating kompo_fs usage with Mutex)
// ============================================================================

fn bench_concurrent_stat(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_stat");

    // Test with different thread counts
    for num_threads in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller0.rb")],
                    vec![OsStr::new("config"), OsStr::new("routes.rb")],
                    vec![
                        OsStr::new("vendor"), OsStr::new("bundle"), OsStr::new("ruby"),
                        OsStr::new("3.2.0"), OsStr::new("gems"), OsStr::new("rails"),
                        OsStr::new("lib"), OsStr::new("rails0.rb"),
                    ],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];
                                let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                // Acquire lock for each stat call (simulating real usage)
                                let fs = fs.lock().unwrap();
                                fs.stat(black_box(path), &mut stat_buf)
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_concurrent_require(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_require");

    // Simulate multiple threads requiring files (like Ruby's require)
    for num_threads in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model1.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model2.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model3.rb")],
                    vec![OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller0.rb")],
                    vec![OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller1.rb")],
                    vec![OsStr::new("config"), OsStr::new("routes.rb")],
                    vec![OsStr::new("config"), OsStr::new("application.rb")],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];

                                // stat
                                {
                                    let fs = fs.lock().unwrap();
                                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                    fs.stat(black_box(path), &mut stat_buf);
                                }

                                // open
                                let fd = {
                                    let fs = fs.lock().unwrap();
                                    fs.open(path).unwrap()
                                };

                                // read
                                {
                                    let fs = fs.lock().unwrap();
                                    let mut buf = [0u8; 8192];
                                    while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                                }

                                // close
                                {
                                    let fs = fs.lock().unwrap();
                                    fs.close(fd);
                                }
                                unsafe { libc::close(fd) };
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_concurrent_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_mixed");

    // Mixed workload: some threads do stat, some do full require cycle
    for num_threads in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let stat_paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("config"), OsStr::new("routes.rb")],
                ];
                let require_paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller0.rb")],
                    vec![OsStr::new("lib"), OsStr::new("lib0.rb")],
                ];
                let stat_paths = Arc::new(stat_paths);
                let require_paths = Arc::new(require_paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let stat_paths = Arc::clone(&stat_paths);
                            let require_paths = Arc::clone(&require_paths);

                            thread::spawn(move || {
                                if i % 2 == 0 {
                                    // stat-only thread (read-heavy)
                                    for _ in 0..10 {
                                        let path = &stat_paths[i / 2 % stat_paths.len()];
                                        let fs = fs.lock().unwrap();
                                        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                        fs.stat(black_box(path), &mut stat_buf);
                                    }
                                } else {
                                    // require thread (read + write)
                                    let path = &require_paths[i / 2 % require_paths.len()];

                                    let fd = {
                                        let fs = fs.lock().unwrap();
                                        fs.open(path).unwrap()
                                    };

                                    {
                                        let fs = fs.lock().unwrap();
                                        let mut buf = [0u8; 8192];
                                        while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                                    }

                                    {
                                        let fs = fs.lock().unwrap();
                                        fs.close(fd);
                                    }
                                    unsafe { libc::close(fd) };
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_lock_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("lock_contention");

    // Compare single-threaded with vs without Mutex overhead
    group.bench_function("without_mutex", |b| {
        let fs = create_rails_app_fs();
        let path: Vec<&OsStr> = vec![
            OsStr::new("app"),
            OsStr::new("models"),
            OsStr::new("model0.rb"),
        ];
        b.iter(|| {
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf)
        })
    });

    group.bench_function("with_mutex_uncontended", |b| {
        let fs = Mutex::new(create_rails_app_fs());
        let path: Vec<&OsStr> = vec![
            OsStr::new("app"),
            OsStr::new("models"),
            OsStr::new("model0.rb"),
        ];
        b.iter(|| {
            let fs = fs.lock().unwrap();
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf)
        })
    });

    group.bench_function("with_rwlock_uncontended", |b| {
        let fs = RwLock::new(create_rails_app_fs());
        let path: Vec<&OsStr> = vec![
            OsStr::new("app"),
            OsStr::new("models"),
            OsStr::new("model0.rb"),
        ];
        b.iter(|| {
            let fs = fs.read().unwrap();
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            fs.stat(black_box(&path), &mut stat_buf)
        })
    });

    group.finish();
}

// ============================================================================
// RwLock benchmarks (comparing Mutex vs RwLock for concurrent access)
// After implementing internal RwLock, change Arc<RwLock<Fs>> to Arc<Fs>
// ============================================================================

/// Benchmark concurrent stat with RwLock (read lock only)
/// stat is read-only, so RwLock should allow parallel reads
fn bench_rwlock_stat(c: &mut Criterion) {
    let mut group = c.benchmark_group("rwlock_stat");

    for num_threads in [1, 2, 4, 8] {
        // Mutex baseline
        group.bench_with_input(
            BenchmarkId::new("mutex", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller0.rb")],
                    vec![OsStr::new("config"), OsStr::new("routes.rb")],
                    vec![
                        OsStr::new("vendor"), OsStr::new("bundle"), OsStr::new("ruby"),
                        OsStr::new("3.2.0"), OsStr::new("gems"), OsStr::new("rails"),
                        OsStr::new("lib"), OsStr::new("rails0.rb"),
                    ],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];
                                let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                let fs = fs.lock().unwrap();
                                fs.stat(black_box(path), &mut stat_buf)
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );

        // RwLock (read lock for stat)
        group.bench_with_input(
            BenchmarkId::new("rwlock", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(RwLock::new(create_rails_app_fs()));
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller0.rb")],
                    vec![OsStr::new("config"), OsStr::new("routes.rb")],
                    vec![
                        OsStr::new("vendor"), OsStr::new("bundle"), OsStr::new("ruby"),
                        OsStr::new("3.2.0"), OsStr::new("gems"), OsStr::new("rails"),
                        OsStr::new("lib"), OsStr::new("rails0.rb"),
                    ],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];
                                let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                // RwLock: read lock allows parallel reads
                                let fs = fs.read().unwrap();
                                fs.stat(black_box(path), &mut stat_buf)
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent require with RwLock
/// Simulates: stat (read) -> open (write) -> read (write) -> close (write)
fn bench_rwlock_require(c: &mut Criterion) {
    let mut group = c.benchmark_group("rwlock_require");

    for num_threads in [1, 2, 4, 8] {
        // Mutex baseline
        group.bench_with_input(
            BenchmarkId::new("mutex", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model1.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model2.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model3.rb")],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];

                                // stat
                                {
                                    let fs = fs.lock().unwrap();
                                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                    fs.stat(black_box(path), &mut stat_buf);
                                }

                                // open
                                let fd = {
                                    let fs = fs.lock().unwrap();
                                    fs.open(path).unwrap()
                                };

                                // read
                                {
                                    let fs = fs.lock().unwrap();
                                    let mut buf = [0u8; 8192];
                                    while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                                }

                                // close
                                {
                                    let fs = fs.lock().unwrap();
                                    fs.close(fd);
                                }
                                unsafe { libc::close(fd) };
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );

        // RwLock
        group.bench_with_input(
            BenchmarkId::new("rwlock", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(RwLock::new(create_rails_app_fs()));
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model1.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model2.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model3.rb")],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];

                                // stat (read lock)
                                {
                                    let fs = fs.read().unwrap();
                                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                    fs.stat(black_box(path), &mut stat_buf);
                                }

                                // open (write lock)
                                let fd = {
                                    let fs = fs.write().unwrap();
                                    fs.open(path).unwrap()
                                };

                                // read (write lock)
                                {
                                    let fs = fs.write().unwrap();
                                    let mut buf = [0u8; 8192];
                                    while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                                }

                                // close (write lock)
                                {
                                    let fs = fs.write().unwrap();
                                    fs.close(fd);
                                }
                                unsafe { libc::close(fd) };
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

/// Benchmark read-heavy workload (many stat calls vs few require calls)
/// This is where RwLock should shine: many readers can proceed in parallel
fn bench_rwlock_read_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("rwlock_read_heavy");

    // 8 threads: 7 doing stat (read), 1 doing require (write)
    let num_threads = 8;
    let stat_threads = 7;

    group.bench_function("mutex", |b| {
        let fs = Arc::new(Mutex::new(create_rails_app_fs()));
        let stat_path: Vec<&'static OsStr> = vec![
            OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb"),
        ];
        let require_path: Vec<&'static OsStr> = vec![
            OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller0.rb"),
        ];
        let stat_path = Arc::new(stat_path);
        let require_path = Arc::new(require_path);

        b.iter(|| {
            let handles: Vec<_> = (0..num_threads)
                .map(|i| {
                    let fs = Arc::clone(&fs);
                    let stat_path = Arc::clone(&stat_path);
                    let require_path = Arc::clone(&require_path);

                    thread::spawn(move || {
                        if i < stat_threads {
                            // stat-only threads (read-heavy)
                            for _ in 0..20 {
                                let fs = fs.lock().unwrap();
                                let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                fs.stat(black_box(&stat_path), &mut stat_buf);
                            }
                        } else {
                            // require thread (write)
                            let fd = {
                                let fs = fs.lock().unwrap();
                                fs.open(&require_path).unwrap()
                            };

                            {
                                let fs = fs.lock().unwrap();
                                let mut buf = [0u8; 8192];
                                while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                            }

                            {
                                let fs = fs.lock().unwrap();
                                fs.close(fd);
                            }
                            unsafe { libc::close(fd) };
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        })
    });

    group.bench_function("rwlock", |b| {
        let fs = Arc::new(RwLock::new(create_rails_app_fs()));
        let stat_path: Vec<&'static OsStr> = vec![
            OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb"),
        ];
        let require_path: Vec<&'static OsStr> = vec![
            OsStr::new("app"), OsStr::new("controllers"), OsStr::new("controller0.rb"),
        ];
        let stat_path = Arc::new(stat_path);
        let require_path = Arc::new(require_path);

        b.iter(|| {
            let handles: Vec<_> = (0..num_threads)
                .map(|i| {
                    let fs = Arc::clone(&fs);
                    let stat_path = Arc::clone(&stat_path);
                    let require_path = Arc::clone(&require_path);

                    thread::spawn(move || {
                        if i < stat_threads {
                            // stat-only threads (read lock - can run in parallel)
                            for _ in 0..20 {
                                let fs = fs.read().unwrap();
                                let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                fs.stat(black_box(&stat_path), &mut stat_buf);
                            }
                        } else {
                            // require thread (write lock)
                            let fd = {
                                let fs = fs.write().unwrap();
                                fs.open(&require_path).unwrap()
                            };

                            {
                                let fs = fs.write().unwrap();
                                let mut buf = [0u8; 8192];
                                while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                            }

                            {
                                let fs = fs.write().unwrap();
                                fs.close(fd);
                            }
                            unsafe { libc::close(fd) };
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        })
    });

    group.finish();
}

/// Benchmark internal RwLock: Arc<Fs> without external lock
/// This tests the internal RwLock implementation directly
fn bench_internal_rwlock(c: &mut Criterion) {
    let mut group = c.benchmark_group("internal_rwlock");

    for num_threads in [2, 4, 8, 16] {
        // External Mutex (baseline - old approach)
        group.bench_with_input(
            BenchmarkId::new("external_mutex_stat", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let path: Vec<&'static OsStr> = vec![
                    OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb"),
                ];
                let path = Arc::new(path);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let fs = Arc::clone(&fs);
                            let path = Arc::clone(&path);
                            thread::spawn(move || {
                                for _ in 0..10 {
                                    let fs = fs.lock().unwrap();
                                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                    fs.stat(black_box(&path), &mut stat_buf);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );

        // Internal RwLock only (new approach - no external lock for stat)
        // stat only accesses trie, no lock needed
        group.bench_with_input(
            BenchmarkId::new("internal_only_stat", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(create_rails_app_fs());
                let path: Vec<&'static OsStr> = vec![
                    OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb"),
                ];
                let path = Arc::new(path);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let fs = Arc::clone(&fs);
                            let path = Arc::clone(&path);
                            thread::spawn(move || {
                                for _ in 0..10 {
                                    // No external lock needed - stat is lock-free (trie only)
                                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                    fs.stat(black_box(&path), &mut stat_buf);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );

        // Require cycle: external mutex vs internal rwlock
        group.bench_with_input(
            BenchmarkId::new("external_mutex_require", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model1.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model2.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model3.rb")],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];
                                let fs = fs.lock().unwrap();

                                // stat -> open -> read -> close (all under single lock)
                                let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                fs.stat(black_box(path), &mut stat_buf);

                                let fd = fs.open(path).unwrap();
                                let mut buf = [0u8; 8192];
                                while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                                fs.close(fd);
                                unsafe { libc::close(fd) };
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );

        // Internal RwLock: no external lock, internal RwLock handles fd_map
        group.bench_with_input(
            BenchmarkId::new("internal_only_require", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(create_rails_app_fs());
                let paths: Vec<Vec<&'static OsStr>> = vec![
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model1.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model2.rb")],
                    vec![OsStr::new("app"), OsStr::new("models"), OsStr::new("model3.rb")],
                ];
                let paths = Arc::new(paths);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let fs = Arc::clone(&fs);
                            let paths = Arc::clone(&paths);
                            thread::spawn(move || {
                                let path = &paths[i % paths.len()];

                                // No external lock - internal RwLock handles concurrency
                                let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                fs.stat(black_box(path), &mut stat_buf);

                                let fd = fs.open(path).unwrap();
                                let mut buf = [0u8; 8192];
                                while fs.read(fd, &mut buf).unwrap_or(0) > 0 {}
                                fs.close(fd);
                                unsafe { libc::close(fd) };
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

/// Benchmark pure stat workload (100% read operations)
/// This is the best case for RwLock: all threads can read in parallel
fn bench_rwlock_stat_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("rwlock_stat_only");

    for num_threads in [2, 4, 8, 16] {
        // Mutex: all threads contend for exclusive lock
        group.bench_with_input(
            BenchmarkId::new("mutex", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(Mutex::new(create_rails_app_fs()));
                let path: Vec<&'static OsStr> = vec![
                    OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb"),
                ];
                let path = Arc::new(path);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let fs = Arc::clone(&fs);
                            let path = Arc::clone(&path);
                            thread::spawn(move || {
                                for _ in 0..10 {
                                    let fs = fs.lock().unwrap();
                                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                    fs.stat(black_box(&path), &mut stat_buf);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );

        // RwLock: all threads can read in parallel
        group.bench_with_input(
            BenchmarkId::new("rwlock", num_threads),
            &num_threads,
            |b, &num_threads| {
                let fs = Arc::new(RwLock::new(create_rails_app_fs()));
                let path: Vec<&'static OsStr> = vec![
                    OsStr::new("app"), OsStr::new("models"), OsStr::new("model0.rb"),
                ];
                let path = Arc::new(path);

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let fs = Arc::clone(&fs);
                            let path = Arc::clone(&path);
                            thread::spawn(move || {
                                for _ in 0..10 {
                                    let fs = fs.read().unwrap();
                                    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
                                    fs.stat(black_box(&path), &mut stat_buf);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_require_simulation,
    bench_dir_glob_simulation,
    bench_read_by_size,
    bench_stat_by_depth,
    bench_scalability,
    bench_basic_operations,
    bench_concurrent_stat,
    bench_concurrent_require,
    bench_concurrent_mixed_workload,
    bench_lock_contention,
    // RwLock vs Mutex comparison benchmarks
    bench_rwlock_stat,
    bench_rwlock_require,
    bench_rwlock_read_heavy,
    bench_rwlock_stat_only,
    // Internal RwLock benchmarks (Arc<Fs> without external lock)
    bench_internal_rwlock,
);
criterion_main!(benches);
