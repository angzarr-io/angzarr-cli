package codegen

// The helper functions every template set may call (docs/templates.md lists
// them). Model values arrive as decoded JSON (string, bool, json.Number,
// []any, map[string]any), so list helpers accept []any as well as []string.

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"text/template"
	"unicode"
)

func baseFuncs() template.FuncMap {
	return template.FuncMap{
		// Case conversion.
		"snake":  snake,
		"camel":  camel,
		"pascal": pascal,
		"upper":  strings.ToUpper,
		"lower":  strings.ToLower,
		// Strings.
		"quote":      quoteLiteral,
		"join":       join,
		"split":      func(sep, s string) []any { return strs(strings.Split(s, sep)) },
		"replace":    func(old, new, s string) string { return strings.ReplaceAll(s, old, new) },
		"trimPrefix": func(prefix, s string) string { return strings.TrimPrefix(s, prefix) },
		"trimSuffix": func(suffix, s string) string { return strings.TrimSuffix(s, suffix) },
		"hasPrefix":  func(prefix, s string) bool { return strings.HasPrefix(s, prefix) },
		"hasSuffix":  func(suffix, s string) bool { return strings.HasSuffix(s, suffix) },
		"contains":   func(sub, s string) bool { return strings.Contains(s, sub) },
		"repeat":     func(n any, s string) (string, error) { i, err := toInt(n); return strings.Repeat(s, max(i, 0)), err },
		// Lists.
		"list": func(items ...any) []any { return items },
		"append": func(l any, items ...any) ([]any, error) {
			s, err := toList(l)
			return append(append([]any{}, s...), items...), err
		},
		"first":        func(l any) (any, error) { return at(l, 0) },
		"last":         func(l any) (any, error) { return at(l, -1) },
		"initial":      func(l any) ([]any, error) { s, err := toList(l); return dropEnd(s), err },
		"rest":         func(l any) ([]any, error) { s, err := toList(l); return dropStart(s), err },
		"sortStrings":  sortStrings,
		"commonPrefix": commonPrefix,
		"has":          has,
		// Arithmetic.
		"add": func(a, b any) (int, error) {
			x, err := toInt(a)
			y, err2 := toInt(b)
			return x + y, firstErr(err, err2)
		},
		"sub": func(a, b any) (int, error) {
			x, err := toInt(a)
			y, err2 := toInt(b)
			return x - y, firstErr(err, err2)
		},
		// Maps.
		"dict":   dict,
		"get":    func(m map[string]any, key string) any { return m[key] },
		"set":    func(m map[string]any, key string, v any) string { m[key] = v; return "" },
		"hasKey": func(m map[string]any, key string) bool { _, ok := m[key]; return ok },
		// Control.
		"fail": func(msg string) (string, error) { return "", fmt.Errorf("%s", msg) },
		// Render-scoped (rebound per component; see TemplateSet.bind).
		"include": func(string, any) (string, error) { return "", fmt.Errorf("include is unavailable outside a render") },
		"typeRef": func(any) (string, error) { return "", fmt.Errorf("typeRef is unavailable outside a render") },
	}
}

// snake converts a CamelCase identifier to snake_case. An upper-case letter
// starts a new word after a lower-case letter, or ends an acronym run when a
// lower-case letter follows it (HTTPGet → http_get).
func snake(name string) string {
	var b strings.Builder
	for i, r := range name {
		if unicode.IsUpper(r) {
			if i > 0 && (!unicode.IsUpper(rune(name[i-1])) || (i+1 < len(name) && unicode.IsLower(rune(name[i+1])))) {
				b.WriteByte('_')
			}
			b.WriteRune(unicode.ToLower(r))
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// pascal upper-cases the first letter and the letter after each underscore,
// dropping the underscores (order_created → OrderCreated); a PascalCase input
// is returned unchanged.
func pascal(name string) string {
	var b strings.Builder
	upNext := true
	for _, r := range name {
		if r == '_' {
			upNext = true
			continue
		}
		if upNext {
			b.WriteRune(unicode.ToUpper(r))
			upNext = false
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// camel is pascal with the first letter lower-cased (OrderCreated →
// orderCreated).
func camel(name string) string {
	p := []rune(pascal(name))
	if len(p) > 0 {
		p[0] = unicode.ToLower(p[0])
	}
	return string(p)
}

func strs(in []string) []any {
	out := make([]any, len(in))
	for i, s := range in {
		out[i] = s
	}
	return out
}

func toList(l any) ([]any, error) {
	switch v := l.(type) {
	case nil:
		return nil, nil
	case []any:
		return v, nil
	case []string:
		return strs(v), nil
	default:
		return nil, fmt.Errorf("want a list, got %T", l)
	}
}

func toInt(v any) (int, error) {
	switch n := v.(type) {
	case int:
		return n, nil
	case json.Number:
		i, err := n.Int64()
		return int(i), err
	default:
		return 0, fmt.Errorf("want an integer, got %T", v)
	}
}

func firstErr(errs ...error) error {
	for _, err := range errs {
		if err != nil {
			return err
		}
	}
	return nil
}

func join(sep string, l any) (string, error) {
	items, err := toList(l)
	if err != nil {
		return "", err
	}
	parts := make([]string, len(items))
	for i, it := range items {
		parts[i] = fmt.Sprint(it)
	}
	return strings.Join(parts, sep), nil
}

func at(l any, i int) (any, error) {
	s, err := toList(l)
	if err != nil {
		return nil, err
	}
	if len(s) == 0 {
		return nil, fmt.Errorf("empty list")
	}
	if i < 0 {
		i += len(s)
	}
	return s[i], nil
}

func dropEnd(s []any) []any {
	if len(s) == 0 {
		return []any{}
	}
	return s[:len(s)-1]
}

func dropStart(s []any) []any {
	if len(s) == 0 {
		return []any{}
	}
	return s[1:]
}

func sortStrings(l any) ([]any, error) {
	s, err := toList(l)
	if err != nil {
		return nil, err
	}
	parts := make([]string, len(s))
	for i, it := range s {
		parts[i] = fmt.Sprint(it)
	}
	sort.Strings(parts)
	return strs(parts), nil
}

// commonPrefix is the number of leading elements two lists share.
func commonPrefix(a, b any) (int, error) {
	x, err := toList(a)
	if err != nil {
		return 0, err
	}
	y, err := toList(b)
	if err != nil {
		return 0, err
	}
	n := 0
	for n < len(x) && n < len(y) && fmt.Sprint(x[n]) == fmt.Sprint(y[n]) {
		n++
	}
	return n, nil
}

// has reports whether a list holds an element equal to v.
func has(l any, v any) (bool, error) {
	s, err := toList(l)
	if err != nil {
		return false, err
	}
	for _, it := range s {
		if fmt.Sprint(it) == fmt.Sprint(v) {
			return true, nil
		}
	}
	return false, nil
}

func dict(kv ...any) (map[string]any, error) {
	if len(kv)%2 != 0 {
		return nil, fmt.Errorf("dict wants key/value pairs, got %d arguments", len(kv))
	}
	m := make(map[string]any, len(kv)/2)
	for i := 0; i < len(kv); i += 2 {
		k, ok := kv[i].(string)
		if !ok {
			return nil, fmt.Errorf("dict key %v is %T, want string", kv[i], kv[i])
		}
		m[k] = kv[i+1]
	}
	return m, nil
}
