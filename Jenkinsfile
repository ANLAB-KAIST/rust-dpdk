pipeline {
    agent { dockerfile true }
    environment {
        CARGO = "cargo --locked"
        RTE_SDK = "/usr/local/share/dpdk"
    }
    stages {
        stage ("Version") {
            steps {
                sh "$CARGO --version"
                sh "rustc --version"
                sh "rustup component add rustfmt"
                sh "rustup component add clippy"
            }
        }
        stage ("Check") {
            steps {
                sh "$CARGO check"
                sh "$CARGO fmt --all -- --check"
                sh "$CARGO clippy -- -D warnings"
            }
        }
        stage ("Build") {
            steps {
                sh "$CARGO build"
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
