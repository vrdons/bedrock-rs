cargo build --example forwarder --target x86_64-linux-android

waydroid adb connect
adb push target/x86_64-linux-android/debug/bedrock /data/local/tmp/
adb shell chmod +x /data/local/tmp/bedrock
adb shell /data/local/tmp/bedrock