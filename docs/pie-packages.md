# PIE package catalog

Useful native extensions available through [PIE](https://github.com/php/pie).
Package names and stable-release PHP constraints were checked against
[Packagist](https://packagist.org/extensions) on 2026-11-21.
This is a curated starting list, not a popularity ranking or support guarantee.

**No package below has been verified with these bundles.** Redis is the
planned build-and-load check across the experimental PHP matrix. Other entries
are catalog references only. A declared PHP constraint does not prove that the
extension builds against a newly released PHP version or on both platforms.

| Package and repository | Purpose | Stable release metadata checked | Additional requirements or limits |
| --- | --- | --- | --- |
| [`phpredis/phpredis`](https://github.com/phpredis/phpredis) | Redis client, sessions, and caching | 6.3.0; PHP >=7.4 | A Redis server is needed to use the client, not to check that the extension loads. Optional compression and serialization features add dependencies. |
| [`apcu/apcu`](https://github.com/krakjoe/apcu) | In-process user-data cache | 5.1.28; PHP >=7.0 | CLI caching is disabled by default; enable `apc.enable_cli` when testing CLI cache operations. |
| [`xdebug/xdebug`](https://github.com/xdebug/xdebug) | Step debugging, profiling, and coverage | 3.5.3; PHP >=8.0, <8.6 | This stable version excludes PHP 8.6. Load as a Zend extension; do not substitute an untested development build. |
| [`imagick/imagick`](https://github.com/Imagick/imagick) | Image processing through ImageMagick | 3.8.1; PHP >=5.6 | Requires compatible ImageMagick libraries and development files. Available formats depend on its delegates and security policy. |
| [`mongodb/mongodb-extension`](https://github.com/mongodb/mongo-php-driver) | Native MongoDB driver | 2.5.8; PHP >=8.1, <9 | Applications commonly also use the separate `mongodb/mongodb` PHP library. A server is needed for database operations. |
| [`php-memcached/php-memcached`](https://github.com/php-memcached-dev/php-memcached) | Memcached client and session handler | 3.4.0; PHP >=7.0 | Requires libmemcached development files; SASL and optional serializers add dependencies. A server is needed for cache operations. |
| [`pecl/pcov`](https://github.com/krakjoe/pcov) | Lightweight line coverage | 1.0.12; PHP >=7.1 | Check upstream limitations before enabling it alongside another coverage engine. |
| [`open-telemetry/ext-opentelemetry`](https://github.com/open-telemetry/opentelemetry-php-instrumentation) | Hooks for automatic instrumentation | 1.5.0; PHP ^8.0 | The extension alone does not export telemetry; applications need the corresponding PHP instrumentation and SDK. |

## Targeting an extracted PHP bundle

Install the required compiler and libraries before invoking PIE. Use the bundle's
PHP to run PIE, put its `bin` directory first on `PATH`, and explicitly select its
`php-config` and `phpize` with PIE's `--with-php-config` and `--with-phpize-path`
options. This avoids accidentally building an extension for another PHP.

For example, the package argument for Redis is `phpredis/phpredis`; Xdebug uses
`xdebug/xdebug`. PIE chooses a package version compatible with the target PHP,
subject to the package's own metadata and available releases. Review the
resulting INI configuration and verify the loaded extension with that same PHP.

System packages, Homebrew formulae, extension source builds, and PIE's offers to
install missing tools are separate installation operations. Review them before
proceeding; the catalog does not authorize automatic tool installation.
