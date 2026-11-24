# PHP builds

Experimental prebuilt PHP distributions for Linux and macOS with PIE extension support.

> [!WARNING]
> No successful build or PIE installation has been established yet. This repository
> prepares the experiment; it does not provide production-ready PHP downloads.

## First experiment

Build the latest stable patch release in each PHP line from **8.1 through 8.6**,
with matching development headers, `phpize`, `php-config`, and build support files.
PIE uses these files to compile an extension without rebuilding PHP itself.

| Build target | Architecture |
| --- | --- |
| Ubuntu 22.04, Ubuntu 24.04 | x86_64 |
| Debian bookworm, Debian trixie | x86_64 |
| Fedora latest | x86_64 |
| CentOS Stream 9, CentOS Stream 10 | x86_64 |
| Rocky Linux 8, 9, 10 | x86_64 |
| AlmaLinux 8, 9, 10 | x86_64 |
| Oracle Linux 8, 9, 10 | x86_64 |
| Amazon Linux 2023 | x86_64 |
| Arch Linux | x86_64 |
| Alpine latest | x86_64, musl |
| macOS 15 | arm64 |
| macOS 26 | arm64 |

Every Linux target is built inside its own distribution image, because the
archives link that distribution's OpenSSL, libxml2, ICU and readline. An
archive is therefore not portable between distributions. macOS builds link
Homebrew library locations.
Linux arm64, macOS Intel, and Windows are outside this first experiment.
Fedora and Arch Linux ship OpenSSL 4.x, which PHP before 8.4.28 cannot compile against, so those two targets build only PHP 8.4.28 and newer. PHP 8.1 is end-of-life; an experimental build does not restore security support.
Future PHP lines can be added to the matrix separately.

## Extension coverage

Each build enables what a typical Laravel, Symfony or WordPress project needs:
MySQL/MariaDB (`mysqli`, `pdo_mysql`, `mysqlnd`), PostgreSQL (`pgsql`,
`pdo_pgsql`), SQLite, ODBC, SQL Server through FreeTDS (`pdo_dblib`), `dba`,
`bcmath`, `gmp`, `sodium`, `intl`, `gd`, `zip`, `xsl`, `tidy`, `ldap`, `snmp`,
`soap`, `exif`, `gettext`, the `pcntl`/sysv family, and opcache. Oracle
(`oci8`, `pdo_oci`) is not included because it requires the non-redistributable
Oracle Instant Client. Third-party extensions such as Redis, Xdebug or Imagick
are installed later with PIE.

## Build and verification

The workflow is **manual only**. Opening or updating a PR does not install tools
or run builds. Before a manual run, approve the tools and download sources:

- Ubuntu distribution packages or Homebrew formulae on disposable hosted runners.
- PHP release metadata and source archives from `php.net`, with SHA-256 checks.
- The pinned PIE PHAR from the official `php/pie` GitHub release, with a SHA-256 check.

Dependency installation uses a vendored copy of the mise-php dependency script,
which handles each distribution's package names, RHEL EPEL/CRB repositories, and
macOS Homebrew formulae.

The hosted jobs build the `php-builds` binary first and cache the Cargo registry
and `target` directory with `actions/cache`. Container jobs install Rust with
rustup and rebuild the binary, because the distribution images ship neither
Node.js nor a cache backend for the action. Each job then builds and packages PHP, extracts the
archive at a different prefix, removes the original installation, and compiles
and loads Redis through PIE using the relocated toolchain. The extension check
uses a disposable copy, leaving the distributed PHP archive free of that
verification extension.

Each build uploads its archive straight into a **draft** release for that PHP
version instead of a workflow artifact. A second job per target then downloads
that archive on its own operating system, extracts it under a different prefix,
and repeats the relocation and PIE checks, so the published file itself is what
gets tested rather than a build directory. Only when every target passes does a
final job write the release's single `SHA256SUMS`, attach it, and flip the draft
to a prerelease. A failed target leaves the release unpublished.

Nothing is rebuilt on a schedule. The cache covers the Rust binary only, not PHP
itself.

A matrix stops at the first failure so a broken combination does not consume the
remaining targets.

A manual run can be narrowed with the `php_minor` and `target` inputs, so one
line or one platform can be validated before the full matrix is exercised.

## Releases

Releases are opt-in per run: set the `release` input and a single `php_minor`.
The run then publishes a prerelease tagged `php-<version>`, containing one
archive per build target plus the generated manifest and checksums. Re-running
the same version uploads the assets again. Releases stay marked as prereleases
until a build has been validated on the target platform.

## Local builds

The same binary builds PHP outside CI once the platform toolchain and the
development libraries listed above are installed:

```sh
cargo build --release --locked
./target/release/php-builds resolve --minor 8.5
./target/release/php-builds build --minor 8.5 --root ./out
./target/release/php-builds verify --package ./out/package --minor 8.5
```

`build` downloads the release archive from `php.net`, verifies its SHA-256, and
fails if the compiler, `make`, or a required library is missing. `verify`
downloads the pinned PIE PHAR and installs Redis into a disposable copy.
Local builds have not been exercised outside the two CI targets.

PHP and dependency versions can change between manual runs. Keep the generated
metadata and checksums with an archive when comparing results.

## Runtime and development dependencies

These are ordinary **system-linked** builds, not StaticPHP or self-contained
bundles. The target machine must provide compatible shared libraries; macOS
builds depend on Homebrew library locations. Moving the PHP prefix does not
make those external libraries portable.

Building extensions also requires a compiler, `make`, Autoconf, and any
extension-specific development libraries. Matching PHP headers and build tools
are included; the full PHP source tree is not needed. Prefixes and extension
build directories containing whitespace are unsupported by upstream `phpize`.

The [PIE package catalog](docs/pie-packages.md) lists useful extensions, their
upstream repositories, and compatibility caveats. Package availability is not
evidence that a package has passed this experiment.

Custom PHP configure flags, additional bundled extensions, dependency bundling,
and integration with `mise-php` remain separate work.

## Licensing

Repository code is AGPL-3.0-or-later; see [LICENSE](LICENSE). PHP and its bundled
components retain their own licenses. Experimental archives include upstream
PHP license notices; system libraries and separately installed PIE extensions
have their own terms.
