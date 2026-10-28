# Messages for angzarr-cli's diagnostics. Each id is a diagnostic's code; the
# variables are its args.

angzarr-cli-translate-malformed = --translate { $value } is not <policy>:<command>=<fact>

## Remedies.

angzarr-cli-translate-form = write --translate as <policy>:<command>=<fact>, each a fully qualified message name, e.g. buyin.v1.BuyInWalletPolicy:buyin.v1.Hold=buyin.v1.HoldLost
