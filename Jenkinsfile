pipeline {
    agent { dockerfile true }
    environment {
        CARGO="$HOME/.cargo/bin/cargo --locked"
        RUSTC="$HOME/.cargo/bin/rustc"
    }
    stages {
        stage ("Version") {
            steps {
                sh "$CARGO --version"
                sh "$RUSTC --version"
            }
        }
        stage ("Build") {
            steps {
                sh "$CARGO build"
            }
        }
        stage ("Check") {
            steps {
                sh "$CARGO check"
                sh "$CARGO fmt --all -- --check"
                sh "$CARGO clippy -- -D warnings"
            }
        }
        stage ("Test (common)") {
            steps {
                sh "$CARGO test --lib"
            }
        }
        stage ("Test (dpdk-sys)") {
            steps {
                sh "$CARGO run -p rust-dpdk-sys -- --no-pci --no-huge"
            }
        }
    }
}
