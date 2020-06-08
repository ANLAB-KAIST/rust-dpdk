pipeline {
    agent {
        dockerfile {
            filename 'Dockerfile'
            args '--privileged -v /mnt/huge:/mnt/huge'
        }
    }
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
                sh 'cargo check'
                sh 'cargo fmt --all -- --check'
                sh 'cargo clippy -- -D warnings'
            }
        }
        stage ("Build") {
            steps {
                sh "cargo build"
            }
        }
        stage ("Test (common)") {
            steps {
                sh "cargo test --lib"
            }
        }
        stage ("Test (dpdk-sys)") {
            steps {
                sh "cargo run -p rust-dpdk-sys"
            }
        }
    }
}
