pipeline {
    agent none
    stages {
        stage ("Multi-environment test") {
            parallel {
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
                    agent { dockerfile true }
                    environment {
                        RTE_SDK = '/dpdk'
                        RTE_TARGET = 'build'
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
    }
}