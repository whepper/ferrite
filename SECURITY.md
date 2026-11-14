# Security policy

Ferrite is a small open-source audio project. If you find a security
vulnerability, please report it responsibly so it can be fixed before
public disclosure.

## Reporting

- If GitHub private vulnerability reporting is enabled for this
  repository, use **Security → Report a vulnerability**. This keeps the
  report private until a fix is available.
- Otherwise, open a public issue only if the problem cannot be exploited
  to harm users (for example, a panic on malformed input with no other
  impact). When in doubt, do not open a public issue with exploit
  details; describe the issue class without a working exploit.

## Scope

In scope: the `ferrite-aac` encoder library and the `aac-convert`
application as built from this repository, including dependency or
build-script issues that affect a default `cargo build --locked`.

Out of scope: the private FDK-AAC reference checkout and Tier-B oracle
tooling (not part of this repository), third-party crates (report
upstream), and deployment or operational security of downstream users.

## What to include

- Ferrite version or commit, toolchain (`rustc --version`), and platform.
- Steps to reproduce, including input files where relevant.
- Observed impact and, if known, the affected component.

No response-time or support commitment is made; reports are handled on
a best-effort basis by the maintainer.
