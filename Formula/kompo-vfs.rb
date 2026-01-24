class KompoVfs < Formula
  desc "Virtual filesystem library for kompo gem"
  homepage "https://github.com/ahogappa/kompo-vfs"
  url "https://github.com/ahogappa/kompo-vfs.git", using: :git, branch: "main"
  head "https://github.com/ahogappa/kompo-vfs.git", branch: "main"
  version "0.5.0"

  depends_on "rust" => :build

  def install
    system "cargo build --release"

    lib.install "target/release/libkompo_fs.a"
    lib.install "target/release/libkompo_wrap.a"

    # Write version file for kompo gem compatibility check
    (lib/"KOMPO_VFS_VERSION").write version.to_s
  end

  test do
    system "file", lib/"libkompo_fs.a"
    system "file", lib/"libkompo_wrap.a"
  end
end
