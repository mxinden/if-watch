# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.1]

### Fixed
- Update to `rtnetlink` `v0.10`. See [PR 19].

[PR 19]: https://github.com/mxinden/if-watch/pull/19

## [1.1.0]
### Added
- Return socket closure as error. See [PR 15].

### Fixed
- Update to `windows` `v0.34`. See [PR 16].

[PR 15]: https://github.com/mxinden/if-watch/pull/15
[PR 16]: https://github.com/mxinden/if-watch/pull/16

## [1.0.0] - 2022-01-12
### Added
- macos/ios backend

### Changed
- linux backend rewritten to use rtnetlink
- windows backend rewritten to use windows crate instead of winapi
