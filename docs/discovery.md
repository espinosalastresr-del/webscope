# Discovery Engine

The user never supplies an initial URL or search dork.

## Providers

- **SeedProvider** – small set of public bootstrap hosts.
- **Path patterns** – `/checkout`, `/donate`, `/login`, … expanded onto known hostnames.

## Adaptive ranking

Patterns store uses, relevant hits, false positives and a **precision-weighted score**. Discovery expands higher-scoring paths first. Persistently low-precision patterns are demoted or discarded.

## Allowed sources

Common Crawl, public domain lists and authorised datasets can be added as additional `DiscoveryProvider` implementations. Google Search scraping that evades protections is explicitly out of scope.
