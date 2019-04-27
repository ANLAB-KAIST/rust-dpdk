pipeline {
    agent {
        dockerfile {
            filename "Dockerfile.test"
        }
    }
    stages {
        stage ("Version") {
            steps {
                sh "cargo --version"
                sh "rustc --version"
                sh "rustup component add rustfmt"
            }
        }
        stage ("Check") {
            steps {
                sh "rustfmt --check build.rs"
            }
        }
        stage ("Build") {
            steps {
                sh "cargo build"
            }
        }
    }
}