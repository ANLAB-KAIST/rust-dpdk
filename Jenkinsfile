pipeline {
    agent none
    stages {
        stage ("Debian install") {
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
                        sh "cargo run"
                    }
                }
            }
        }
        stage ("Manual install") {
            agent {
                dockerfile {
                    filename "Dockerfile"
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
                        sh "cargo run"
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
                        sh "cargo run"
                    }
                }
            }
        }
    }
}