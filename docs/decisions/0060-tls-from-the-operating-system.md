# 0060. Networking takes its TLS from the operating system

Status: Accepted
Date: 2026-11-13
Roadmap: D.22, 3.30

## Context

Row 3.30 adds an HTTP and HTTPS client. HTTPS needs TLS. The project builds native Baryon
libraries first, but TLS is a security project of its own: a subtle mistake in it is worse than
having no TLS, and it needs expert review and constant patching.

## Decision

TLS comes from the operating system: OpenSSL on Linux and Apple's Security framework on macOS.
Collins chose this on 2026-11-13.

## Consequences

- Security fixes reach Baryon programs through operating system updates.
- The runtime makes foreign calls into a system library, as the opt-in `--blas` option does, so
  the networking work needs those calls and a test on each platform.
- This is an exception to native first, limited to security code.
