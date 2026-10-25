class Poopterm < Formula
  desc "Small, poop-colored terminal emulator"
  homepage "https://github.com/poopware-laboratories/poopterm"
  url "https://github.com/poopware-laboratories/poopterm.git",
      tag: "v26.10.98",
      using: :git
  version "26.10.98"
  license "AGPL-3.0-or-later"

  depends_on "rustup" => :build

  def install
    ENV["RUSTUP_HOME"] = (buildpath/".rustup").to_s
    ENV["CARGO_HOME"] = (buildpath/".cargo").to_s
    system "rustup", "toolchain", "install", "nightly", "--profile", "minimal"
    system "rustup", "run", "nightly", "cargo", "build", "--release", "--locked"
    unless OS.mac?
      bin.install "target/release/poopterm"
      (share/"doc/poopterm").install "THIRD_PARTY_NOTICES"
      return
    end

    app = prefix/"Poopterm.app"
    macos = app/"Contents/MacOS"
    resources = app/"Contents/Resources"
    macos.mkpath
    resources.mkpath
    (macos/"Poopterm").write (buildpath/"target/release/poopterm").binread
    (resources/"Poopterm.icns").write (buildpath/"assets/poopterm.icns").binread
    (resources/"THIRD_PARTY_NOTICES").write (buildpath/"THIRD_PARTY_NOTICES").read
    (app/"Contents/Info.plist").write <<~PLIST
      <?xml version="1.0" encoding="UTF-8"?>
      <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
      <plist version="1.0">
      <dict>
        <key>CFBundleExecutable</key><string>Poopterm</string>
        <key>CFBundleIdentifier</key><string>labs.poopterm.app</string>
        <key>CFBundleName</key><string>Poopterm</string>
        <key>CFBundleDisplayName</key><string>Poopterm</string>
        <key>CFBundlePackageType</key><string>APPL</string>
        <key>CFBundleIconFile</key><string>Poopterm.icns</string>
        <key>CFBundleVersion</key><string>#{version}</string>
        <key>CFBundleShortVersionString</key><string>#{version}</string>
        <key>LSMinimumSystemVersion</key><string>11.0</string>
        <key>NSHighResolutionCapable</key><true/>
      </dict>
      </plist>
    PLIST
    chmod 0755, macos/"Poopterm"
    bin.install "target/release/poopterm"
    (share/"doc/poopterm").install "THIRD_PARTY_NOTICES"
  end

  test do
    assert_predicate bin/"poopterm", :executable?
    assert_equal "poopterm #{version}", shell_output("#{bin}/poopterm --version").strip
    if OS.mac?
      assert_predicate prefix/"Poopterm.app/Contents/MacOS/Poopterm", :executable?
      assert_predicate prefix/"Poopterm.app/Contents/Resources/Poopterm.icns", :exist?
    end
  end

  def caveats
    return unless OS.mac?

    "The app bundle is installed at #{opt_prefix}/Poopterm.app. Open it with:\n  open #{opt_prefix}/Poopterm.app"
  end
end
