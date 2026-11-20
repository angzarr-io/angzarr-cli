# Messages for this crate's diagnostics. Each id is a diagnostic's code
# without the crate's name, which the build prefixes
# (angzarr_diagnostic::build); the variables are its args.

domain-required = scaffold python needs --domain
expose-malformed = --expose { $value } is not <domain>=loopback or <domain>=ip
language-malformed = --language { $value } is not <name>=<language>
lints = { $config } has { $count } lints
probe-not-listed = { $config } lists no component "{ $component }"
probe-no-input = component "{ $component }" is a { $kind }, which the probe hands an input: name one
probe-payload-unreadable = the payload { $path } could not be read ({ $kind })
probe-failed = component "{ $component }" failed the probe at its { $step } step

## Remedies: an error's code with `-fix` (cli-support looks a remedy up by it).

domain-required-fix = name the domain or the component the project serves, e.g. --domain player or --domain policy-table-player
probe-no-input-fix = name the input: --command <type URL> for a domain, --event <type URL> for a policy or a projector
language-malformed-fix = write --language as <name>=<language>, the component by its name in the application, e.g. --language player=python or --language policy-table-player=python
