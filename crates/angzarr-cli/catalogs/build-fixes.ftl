# The remedies angzarr-build names, under the ids cli-support looks a remedy
# up by: a failure's code with `-fix`. angzarr-build's own catalog holds the
# text under the id of its remedy's code; this aliases it, so the next
# angzarr-build that names its remedy `<code>-fix` makes this file empty.
# (The one catalog that spells another crate's name: the build script does
# not write it.)

angzarr-build-generated-unmarked-fix = { angzarr-build-generated-unmarked-remedy }
