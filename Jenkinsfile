pipeline {
    agent none
    stages {
        stage ("Debian install") {
            agent {
                dockerfile {
                    filename 'Dockerfile.test'
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
        stage ("Manual install") {
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
        stage ("Manual install (env)") {
            agent {
                dockerfile {
                    filename "Dockerfile"
                    args "--env-file=.env_test"
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
    }
}