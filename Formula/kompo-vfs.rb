class KompoVfs < Formula
  desc "Virtual filesystem library for kompo gem"
  homepage "https://github.com/ahogappa0613/kompo-vfs"
  url "https://github.com/ahogappa0613/kompo-vfs.git", using: :git, branch: "main"
  head "https://github.com/ahogappa0613/kompo-vfs.git", branch: "main"
  version "0.2.0"

  depends_on "rust" => :build

  def install
    system "cargo build --release"

    lib.install "target/release/libkompo_fs.a"
    lib.install "target/release/libkompo_wrap.a"
  end

  test do
    system "file", lib/"libkompo_fs.a"
    system "file", lib/"libkompo_wrap.a"
  end
end
