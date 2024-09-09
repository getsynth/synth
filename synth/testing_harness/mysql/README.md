Integration Tests for MySql
====================================

This is an integration test that validates the synth generate and synth import commands for MySql on a Debian flavored
OS. The models in hospital_master are used as a known "golden" set to validate against. The *.sql scripts generate the
schema and test data within the database.

## Note if running locally on Mac OS

`./e2e.sh` assumes the use of the GNU variant of `grep`, specifically in its use of `-P` for PCRE flavored regex.
You will need to run `brew install grep`, and then alter the script to use `ggrep` command instead of the built-in BSD `grep`.

# Requirements

- Docker
- jq

# Instructions

To run this, execute `e2e.sh test-local` script from the current directory. A non-zero return code denotes failure.
Note: run this with all dependencies installed in Vagrant with the [Vagrantfile](tools/vagrant/linux/ubuntu/Vagrantfile)

By default, the script will test MySql. To test MariaDb, set the environment variable `SCHEME=mariadb` before running.
