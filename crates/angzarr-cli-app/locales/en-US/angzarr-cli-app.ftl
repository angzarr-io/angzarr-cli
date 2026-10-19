# Messages for angzarr-cli-app's diagnostics. Each id is a diagnostic's code;
# the variables are its args.

# $reason is the run's refusal, a nested diagnostic rendered in its own words.
angzarr-cli-app-run-refused = the run refused a step: { $reason }
# $dir is the project directory's path.
angzarr-cli-app-project-exists = { $dir } already exists

## Remedies.

angzarr-cli-app-fix-project-exists = edit the settings in { $dir }, or remove it to start over
