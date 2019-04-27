pipeline {
    agent { dockerfile true }
    stages {
        stage ("Version") {
            steps {
                sh "ls -a /"
                sh "ls -a /root"
                sh "ls -a /root/.cargo"
                sh "sync && sleep 1"
                sh "whereis rustup"
                sh "whereis cargo"
                sh "ls -a /root/.cargo/bin"
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