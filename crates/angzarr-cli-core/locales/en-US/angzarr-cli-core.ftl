# Messages for angzarr-cli-core's lint. Each id is a diagnostic's code; the
# variables are its args. $message and $component are fully-qualified proto
# message names; a component is named by its anchor message.

# $marker is the option: command or event.
angzarr-cli-core-unknown-component = { $message }: ({ $marker }).component { $component } is not a component declared in this set
# $kind is a component kind token: aggregate, saga, process-manager, projector
# or unspecified.
angzarr-cli-core-command-not-aggregate = { $message }: (command).component { $component } is a { $kind ->
        [saga] saga
        [process-manager] process manager
        [projector] projector
       *[other] component of unspecified kind
    }; only aggregates handle commands
angzarr-cli-core-emits-unknown = { $message }: (command).emits "{ $emitted }" is not the fully-qualified name of a message in the set
angzarr-cli-core-trigger-without-domain = { $message }: a process-manager trigger for { $component } needs (event).domain
angzarr-cli-core-unspecified-kind = { $component }: (component).kind is not set
# $field is a (component) field name: domain, input_domain or output_domain.
angzarr-cli-core-missing-field = { $component }: { $kind ->
        [aggregate] an aggregate
        [process-manager] a process manager
        [saga] a saga
       *[other] a projector
    } requires (component).{ $field }
# $value is the field's value as declared; several output domains are joined
# with ", ".
angzarr-cli-core-forbidden-field = { $component }: { $kind ->
        [aggregate] an aggregate
        [process-manager] a process manager
        [saga] a saga
       *[other] a projector
    } must not set (component).{ $field } (it is "{ $value }")
# $number is the option's extension field number.
angzarr-cli-core-option-undefined = { $message } carries angzarr option { $number }, but no file in the set defines it
angzarr-cli-core-malformed-option = { $message } carries angzarr option { $number }, but its bytes do not decode
angzarr-cli-core-name-collision = { $component } generates the name { $name }, which { $other } generates too
# $language is a token: every-language, java-typescript or python.
angzarr-cli-core-duplicate-method = { $component } generates the method { $method } twice in { $language ->
        [java-typescript] Java and TypeScript
        [python] Python
       *[other] every language
    } (from { $first } and { $second })
angzarr-cli-core-type-collision = { $component } generates { $generated }, which is already a message or enum in package { $package }
angzarr-cli-core-split-run = { $message } is generated in this run, but its component { $component } is declared in { $anchor_file }, which is not
# $kind is the anchor's kind token, or none when the message anchors nothing.
angzarr-cli-core-state-not-on-state = { $message } carries (state), but it is not the state of an aggregate or a process manager (it is: { $kind })
angzarr-cli-core-unspecified-resolution = { $message }: (state) lists CONFLICT_RESOLUTION_UNSPECIFIED
angzarr-cli-core-no-marked-fields = { $message } lists COMMUTATIVE but marks no field with (conflict)
# $field is a field name, or a oneof's name.
angzarr-cli-core-unmarked-field = { $message }.{ $field } has no (conflict) marker, so it conflicts with every change
angzarr-cli-core-marker-outside-state = { $message }.{ $field } carries (conflict), but { $message } is not a COMMUTATIVE state or reached from one by descend
angzarr-cli-core-descend-non-message = { $message }.{ $field }: descend needs a message field, a map of messages, or a repeated message with element_key
angzarr-cli-core-bad-element-key = { $message }.{ $field }: element_key "{ $key }" needs a repeated message field whose element has a singular scalar, string, bytes or enum field of that name
angzarr-cli-core-ordered-without-key = { $message }.{ $field }: ordered needs element_key
angzarr-cli-core-whole-state-combined = { $message }.{ $field }: whole_state cannot be combined with descend, element_key or ordered
angzarr-cli-core-oneof-member-refined = { $message }.{ $oneof }: oneof members carry only an empty (conflict), or all carry whole_state
angzarr-cli-core-oneof-partially-marked = { $message }.{ $oneof }: some but not all members of the oneof carry (conflict)

## The run's refusals.

angzarr-cli-core-run-model-observed = the model was already observed in this run
angzarr-cli-core-run-no-model = the model must be observed before the lint
angzarr-cli-core-run-linted = this run was already linted

## Remedies.

angzarr-cli-core-fix-one-run = declare the component, and build one image from every file that declares part of it
angzarr-cli-core-fix-command-target = point (command).component at an aggregate's anchor message
angzarr-cli-core-fix-qualify = name each emitted event by the fully-qualified name of a message in the set
angzarr-cli-core-fix-trigger-domain = set (event).domain to the domain the trigger comes from, or set applies to fold the event into the process manager's own state
angzarr-cli-core-fix-kind = set (component).kind to an aggregate, a saga, a process manager or a projector
angzarr-cli-core-fix-set-field = set (component).{ $field }
angzarr-cli-core-fix-clear-field = remove (component).{ $field }
angzarr-cli-core-fix-include-options = build the image with io/angzarr/v1/options.proto and its imports included
angzarr-cli-core-fix-name = set a (component).name distinct from every other component and from every message and enum in the package
angzarr-cli-core-fix-rename-message = rename one of the two messages
angzarr-cli-core-fix-move-state = put (state) on the state message of an aggregate or a process manager
angzarr-cli-core-fix-list-resolution = list CONFLICT_RESOLUTION_COMMUTATIVE, or nothing: STRICT and AGGREGATE_HANDLES are always supported
angzarr-cli-core-fix-mark-field = mark the field with (conflict), or with (conflict) = {"{"}whole_state: true{"}"} when it depends on the rest of the state
angzarr-cli-core-fix-commutative = list CONFLICT_RESOLUTION_COMMUTATIVE in the state's (state) option and reach this message by descend, or remove the marker
angzarr-cli-core-fix-descend = remove descend, or give a repeated message field an element_key
angzarr-cli-core-fix-element-key = name a singular scalar, string, bytes or enum field of the element message
angzarr-cli-core-fix-ordered = set element_key, or remove ordered
angzarr-cli-core-fix-whole-state = use whole_state on its own
angzarr-cli-core-fix-oneof = give every member of the oneof an empty (conflict), or every member whole_state
