package jsonc

import (
	"fmt"
	"math"
	"sort"
	"strconv"
	"unicode"
	"unicode/utf8"
)

// Marshal encodes v as compact standard JSON. Strings escape characters that
// are not printable (control, format, private-use, unassigned, and separator
// runes) as \uXXXX, keeping letters, digits, punctuation, and symbols in their
// original UTF-8 form. HTML-sensitive characters and U+2028/U+2029 are escaped
// to match encoding/json's default output.
func Marshal(v any) ([]byte, error) {
	var dst []byte
	if err := appendJSON(&dst, v); err != nil {
		return nil, err
	}
	return dst, nil
}

func appendJSON(dst *[]byte, v any) error {
	switch x := v.(type) {
	case nil:
		*dst = append(*dst, "null"...)
	case bool:
		if x {
			*dst = append(*dst, "true"...)
		} else {
			*dst = append(*dst, "false"...)
		}
	case string:
		*dst = appendJSONString(*dst, x)
	case float64:
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return fmt.Errorf("json: unsupported value: %s", strconv.FormatFloat(x, 'g', -1, 64))
		}
		*dst = appendFloat(*dst, x)
	case []any:
		*dst = append(*dst, '[')
		for i, item := range x {
			if i > 0 {
				*dst = append(*dst, ',')
			}
			if err := appendJSON(dst, item); err != nil {
				return err
			}
		}
		*dst = append(*dst, ']')
	case map[string]any:
		keys := make([]string, 0, len(x))
		for k := range x {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		*dst = append(*dst, '{')
		for i, k := range keys {
			if i > 0 {
				*dst = append(*dst, ',')
			}
			*dst = appendJSONString(*dst, k)
			*dst = append(*dst, ':')
			if err := appendJSON(dst, x[k]); err != nil {
				return err
			}
		}
		*dst = append(*dst, '}')
	default:
		return fmt.Errorf("json: unsupported type: %T", v)
	}
	return nil
}

const hexDigits = "0123456789abcdef"

func appendJSONString(dst []byte, s string) []byte {
	dst = append(dst, '"')
	for i := 0; i < len(s); {
		c := s[i]
		if c < utf8.RuneSelf {
			switch {
			case c == '"':
				dst = append(dst, '\\', '"')
			case c == '\\':
				dst = append(dst, '\\', '\\')
			case c == '\n':
				dst = append(dst, '\\', 'n')
			case c == '\r':
				dst = append(dst, '\\', 'r')
			case c == '\t':
				dst = append(dst, '\\', 't')
			case c < 0x20, c == '<', c == '>', c == '&':
				dst = appendU4(dst, rune(c))
			default:
				dst = append(dst, c)
			}
			i++
			continue
		}
		r, size := utf8.DecodeRuneInString(s[i:])
		if r == utf8.RuneError && size == 1 {
			dst = append(dst, `\ufffd`...)
			i += size
			continue
		}
		switch {
		case r == '\u2028':
			dst = append(dst, `\u2028`...)
		case r == '\u2029':
			dst = append(dst, `\u2029`...)
		case !unicode.IsPrint(r):
			dst = appendEscapedRune(dst, r)
		default:
			dst = append(dst, s[i:i+size]...)
		}
		i += size
	}
	return append(dst, '"')
}

func appendEscapedRune(dst []byte, r rune) []byte {
	if r < 0x10000 {
		return appendU4(dst, r)
	}
	r -= 0x10000
	dst = appendU4(dst, 0xD800+r>>10)
	return appendU4(dst, 0xDC00+r&0x3FF)
}

func appendU4(dst []byte, r rune) []byte {
	dst = append(dst, '\\', 'u')
	dst = append(dst,
		hexDigits[(r>>12)&0xF],
		hexDigits[(r>>8)&0xF],
		hexDigits[(r>>4)&0xF],
		hexDigits[r&0xF],
	)
	return dst
}

// appendFloat matches encoding/json's number formatting, so unchanged numeric
// literals keep producing the same output as before.
func appendFloat(dst []byte, f float64) []byte {
	abs := math.Abs(f)
	format := byte('f')
	if abs != 0 && (abs < 1e-6 || abs >= 1e21) {
		format = 'e'
	}
	b := strconv.AppendFloat(dst, f, format, -1, 64)
	if format == 'e' {
		n := len(b)
		if n >= 4 && b[n-4] == 'e' && b[n-3] == '-' && b[n-2] == '0' {
			b[n-2] = b[n-1]
			b = b[:n-1]
		}
	}
	return b
}
