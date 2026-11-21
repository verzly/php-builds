# PHP builds

Experimental prebuilt PHP distributions for Linux and macOS with PIE extension support.

> [!WARNING]
> No successful build or PIE installation has been established yet. This repository
> prepares the experiment; it does not provide production-ready PHP downloads.

## First experiment

Build the latest stable patch release in each PHP line from **8.1 through 8.6**,
with matching development headers, `phpize`, `php-config`, and build support files.
PIE uses these files to compile an extension without rebuilding PHP itself.

| Build target | Architecture | Scope |
| --- | --- | --- |
| Ubuntu 22.04 | x86_64 | Native glibc build |
| macOS 15 | arm64 | Native Homebrew-linked build |

These are build targets, not a promise that the archives run on every Linux
distribution or macOS version. Alpine/musl, Linux arm64, macOS Intel, Windows,
and broader distribution validation are outside this first experiment.
PHP 8.1 is end-of-life; an experimental build does not restore security support.
Future PHP lines can be added to the matrix separately.

## Build and verification

The workflow is **manual only**. Opening or updating a PR does not install tools
or run builds. Before a manual run, approve the tools and download sources:

- Ubuntu distribution packages or Homebrew formulae on disposable hosted runners.
- PHP release metadata and source archives from `php.net`, with SHA-256 checks.
- The pinned PIE PHAR from the official `php/pie` GitHub release, with a SHA-256 check.

Each matrix job is intended to build and package PHP, extract the archive at a
different prefix, remove the original installation, and compile and load Redis
through PIE using the relocated toolchain. The extension check uses a disposable
copy, leaving the distributed PHP archive free of that verification extension.
Successful jobs upload experimental workflow artifacts; there is no release
publication or scheduled rebuilding.

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
