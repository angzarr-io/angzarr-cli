# Messages for this crate's diagnostics. Each id is a diagnostic's code
# without the crate's name, which the build prefixes
# (angzarr_diagnostic::build); the variables are its args.

translate-malformed = --translate { $value } is not <policy>:<command>=<fact>
domain-required = scaffold python needs --domain
expose-malformed = --expose { $value } is not <domain>=loopback or <domain>=ip
language-malformed = --language { $value } is not <name>=<language>
lints = { $config } has { $count } lints
probe-not-listed = { $config } lists no component "{ $component }"
probe-no-input = component "{ $component }" is a { $kind }, which the probe hands an input: name one
probe-payload-unreadable = the payload { $path } could not be read ({ $kind })
probe-failed = component "{ $component }" failed the probe at its { $step } step

## Remedies.

translate-form = write --translate as <policy>:<command>=<fact>, each a fully qualified message name, e.g. buyin.v1.BuyInWalletPolicy:buyin.v1.Hold=buyin.v1.HoldLost
domain-name = name the domain or the component the project serves, e.g. --domain player or --domain policy-table-player
probe-input-form = name the input: --command <type URL> for a domain, --event <type URL> for a policy or a projector
language-form = write --language as <name>=<language>, the component by its name in the application, e.g. --language player=python or --language policy-table-player=python
