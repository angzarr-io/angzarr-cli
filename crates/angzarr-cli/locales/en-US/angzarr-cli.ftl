# Messages for angzarr-cli's diagnostics. Each id is a diagnostic's code; the
# variables are its args.

angzarr-cli-translate-malformed = --translate { $value } is not <policy>:<command>=<fact>
angzarr-cli-domain-required = scaffold python needs --domain
angzarr-cli-expose-malformed = --expose { $value } is not <domain>=loopback or <domain>=ip
angzarr-cli-language-malformed = --language { $value } is not <name>=<language>
angzarr-cli-lints = { $config } has { $count } lints
angzarr-cli-probe-not-listed = { $config } lists no component "{ $component }"
angzarr-cli-probe-no-input = component "{ $component }" is a { $kind }, which the probe hands an input: name one
angzarr-cli-probe-payload-unreadable = the payload { $path } could not be read ({ $kind })
angzarr-cli-probe-failed = component "{ $component }" failed the probe at its { $step } step

## Remedies.

angzarr-cli-translate-form = write --translate as <policy>:<command>=<fact>, each a fully qualified message name, e.g. buyin.v1.BuyInWalletPolicy:buyin.v1.Hold=buyin.v1.HoldLost
angzarr-cli-domain-name = name the domain or the component the project serves, e.g. --domain player or --domain policy-table-player
angzarr-cli-probe-input-form = name the input: --command <type URL> for a domain, --event <type URL> for a policy or a projector
angzarr-cli-language-form = write --language as <name>=<language>, the component by its name in the application, e.g. --language player=python or --language policy-table-player=python
