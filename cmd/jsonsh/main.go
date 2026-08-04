package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"jsonsh/internal/jsonc"
	"jsonsh/internal/lang"
)

type options struct {
	expr, script, output             string
	result, compact, pretty, inPlace bool
	maxSteps                         int
}

func main() {
	if err := run(os.Args[1:], os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "jsonsh:", err)
		os.Exit(1)
	}
}
func run(args []string, stdin io.Reader, stdout io.Writer) error {
	var o options
	fs := flag.NewFlagSet("jsonsh", flag.ContinueOnError)
	fs.SetOutput(stdout)
	fs.Usage = func() {
		fmt.Fprintln(stdout, `jsonsh - 使用类 JavaScript 表达式处理 JSON/JSONC

用法:
  jsonsh (-e CODE | -f SCRIPT) [选项] [INPUT]

INPUT 省略时从标准输入读取。支持 //、/* ... */ 注释和尾随逗号。
默认仅替换发生变化的内容，并保留原格式与注释。

脚本:
  -e, --expression CODE  执行指定代码
  -f, --script FILE      从 UTF-8 文件读取代码

根变量:
  $                       当前 JSON 根值
  $ = value               直接替换整个 JSON 根值

输出:
  -r, --result           输出最后一个表达式的值（默认输出修改后的 $）
  -p, --pretty           重新美化输出并保留注释
  -c, --compact          输出紧凑的标准 JSON（移除注释）
  -o, --output FILE      写入指定文件
  -i, --in-place         安全替换输入文件

内置函数:
  length(value)          返回数组元素数、对象属性数或字符串字符数
  has(value, item)       检查数组元素、对象属性或字符串子串是否存在
  keys(value)            返回对象的有序属性名或数组的数字索引

内置函数示例:
  length($.users)
  has($.tags, "go")
  keys($.user)

数组方法:
  array.push(value, ...)  向数组末尾添加一个或多个值，并返回新长度

数组方法示例:
  $.users.push({name: "Tom"})

其他:
      --max-steps N      最大执行步数（默认 1000000）
  -h, -help, --help      显示帮助

示例:
  jsonsh -e "$.price *= 0.8" input.json
  jsonsh -e "length($.users)" -r input.json
  jsonsh -f update.js -i input.json`)
	}
	fs.StringVar(&o.expr, "e", "", "script expression")
	fs.StringVar(&o.expr, "expression", "", "script expression")
	fs.StringVar(&o.script, "f", "", "script file")
	fs.StringVar(&o.script, "script", "", "script file")
	fs.BoolVar(&o.result, "r", false, "output last result")
	fs.BoolVar(&o.result, "result", false, "output last result")
	fs.BoolVar(&o.compact, "c", false, "compact JSON")
	fs.BoolVar(&o.compact, "compact", false, "compact JSON")
	fs.BoolVar(&o.pretty, "p", false, "pretty JSONC")
	fs.BoolVar(&o.pretty, "pretty", false, "pretty JSONC")
	fs.StringVar(&o.output, "o", "", "output file")
	fs.StringVar(&o.output, "output", "", "output file")
	fs.BoolVar(&o.inPlace, "i", false, "replace input file")
	fs.BoolVar(&o.inPlace, "in-place", false, "replace input file")
	fs.IntVar(&o.maxSteps, "max-steps", 1000000, "maximum execution steps")
	if err := fs.Parse(args); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			return nil
		}
		return err
	}
	if (o.expr == "") == (o.script == "") {
		return errors.New("exactly one of -e or -f is required")
	}
	if o.output != "" && o.inPlace {
		return errors.New("-o and -i are mutually exclusive")
	}
	if o.compact && o.pretty {
		return errors.New("--compact and --pretty are mutually exclusive")
	}
	if fs.NArg() > 1 {
		return errors.New("only one input file is supported")
	}
	input := ""
	if fs.NArg() == 1 {
		input = fs.Arg(0)
	}
	if o.inPlace && input == "" {
		return errors.New("-i requires an input file")
	}
	if o.maxSteps <= 0 {
		return errors.New("--max-steps must be positive")
	}
	code := o.expr
	if o.script != "" {
		b, e := os.ReadFile(o.script)
		if e != nil {
			return fmt.Errorf("read script: %w", e)
		}
		code = string(b)
	}
	var rd io.Reader = stdin
	if input != "" {
		f, e := os.Open(input)
		if e != nil {
			return fmt.Errorf("open input: %w", e)
		}
		defer f.Close()
		rd = f
	}
	raw, e := io.ReadAll(rd)
	if e != nil {
		return fmt.Errorf("read input: %w", e)
	}
	doc, e := jsonc.Parse(string(raw))
	if e != nil {
		return e
	}
	root := jsonc.Clone(doc.Root.Value)
	root, last, e := lang.Execute(code, root, o.maxSteps)
	if e != nil {
		return e
	}
	var output string
	if o.result {
		var data []byte
		if o.compact {
			data, e = json.Marshal(last)
		} else {
			data, e = json.MarshalIndent(last, "", "  ")
		}
		output = string(data) + "\n"
	} else if o.compact {
		output, e = jsonc.Compact(root)
		output += "\n"
	} else {
		output, e = doc.Preserve(root)
		if e == nil && o.pretty {
			output, e = jsonc.PrettyPreserve(output, "  ")
			output += "\n"
		}
	}
	if e != nil {
		return fmt.Errorf("encode output: %w", e)
	}
	data := []byte(output)
	if o.inPlace {
		return replaceFile(input, data)
	}
	if o.output != "" {
		return os.WriteFile(o.output, data, 0644)
	}
	_, e = stdout.Write(data)
	return e
}
func replaceFile(path string, data []byte) error {
	dir := filepath.Dir(path)
	f, e := os.CreateTemp(dir, ".jsonsh-*")
	if e != nil {
		return e
	}
	tmp := f.Name()
	ok := false
	defer func() {
		if !ok {
			_ = os.Remove(tmp)
		}
	}()
	if _, e = f.Write(data); e != nil {
		f.Close()
		return e
	}
	if e = f.Sync(); e != nil {
		f.Close()
		return e
	}
	if e = f.Close(); e != nil {
		return e
	}
	backup, e := os.CreateTemp(dir, ".jsonsh-backup-*")
	if e != nil {
		return e
	}
	backupPath := backup.Name()
	if e = backup.Close(); e != nil {
		return e
	}
	if e = os.Remove(backupPath); e != nil {
		return e
	}
	if e = os.Rename(path, backupPath); e != nil {
		return e
	}
	if e = os.Rename(tmp, path); e != nil {
		_ = os.Rename(backupPath, path)
		return e
	}
	_ = os.Remove(backupPath)
	ok = true
	return nil
}
