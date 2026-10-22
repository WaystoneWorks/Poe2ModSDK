# Security policy

## Reporting a vulnerability

Please report security problems **privately**, not in a public issue: use GitHub's private vulnerability reporting for this
repository (the **Security** tab, then **Report a vulnerability**). If that button is not available, open a public issue that only
says you have a security report and ask for a private way to send it; do not put details in the issue.

Include what you found, how to reproduce it (a version or commit, steps) and what you think the impact is. This is a small volunteer
project: expect an answer within about a week and a fix as time allows. Reporters are credited unless they prefer not to be.

## Scope

In scope for **Poe2ModSDK**: the `poe-mod` tool (path handling when packing and verifying, archive limits, anything that writes outside
the folders it is given), the ABI crates (memory-safety problems in the wrappers: a host or mod can cause undefined behaviour through a
documented-as-safe API), and the templates (a generated project that is unsafe by default).

Out of scope: what a mod does once loaded (a mod is native code with the rights of the process it runs in; that is the documented trust
model), Grinding Gear Games' game and services, and using the project against official servers (not supported).

## Supported versions

Only the latest commit on `main` and the latest release get fixes.

## Notes for users

Install only mods you trust. A package's SHA-256 proves the file is the one a server or author published; it does not prove the code is
safe. This project has not had a professional security audit.
