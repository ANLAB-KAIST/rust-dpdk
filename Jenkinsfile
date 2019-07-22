pipeline {
    agent { dockerfile true }
    stages {
        stage ("Version") {
            steps {
                sh "cargo --version"
                sh "rustc --version"
                sh "rustup component add rustfmt"
                sh "rustup component add clippy"
            }
        }
        stage ("Check") {
            steps {
                sh "rustfmt --check build.rs src/test.rs"
                sh "cargo clippy -- -D warnings"
            }
        }
        stage ("Build") {
            steps {
                sh "cargo build"
                sh "cargo run -- --no-pci --no-huge"
            }
        }
    }
}