# Templater

Simple project template manager.

## Install

### Via Cargo

You'll need:
- [rust toolchain](www.rust-lang.org/tools/install)
- [git](git-scm.com/downloads)

```bash
git clone https://github.com/mtrafisz/templater.git
cd templater
cargo install --path .
```

## Usage

### Create a template from directory

```bash
templater create <path>
```

Additional flags:
- `-n`, `--name` - name of the template - this is how You'll later find it.
- `-d`, `--description` - description of the template.
- `-c`, `--command` - add command to template. Commands added to template will be run after creating file system in order they were added. Can be used multiple times.
- `-i`, `--ignore` - ignore files or directories. Ignore patterns are in standard unix glob format. For example `**/*.txt` will ignore all files with `.txt` extension. Can be used multiple times.
- `-r`, `--definition` - provide definition file, instead of typing all template options in one command. See [example definition](examples/raylib-template.tplt)
- `-f`, `--force` - force overwrite existing template.

### Create a project from template

```bash
templater expand <template_name>
```

Additional flags:
- `-a`, `--as` - name of the project. If not provided, name of the project will be the same as the template.
- `-p`, `--path` - path where project will be created. If not provided, project will be created in current directory. Templaters are allways expanded to new, empty directory.
- `-e` `--env` - add envirionment variable to be set, before running template commands. Value of this flag is expected to be "name=value". Can be used multiple times.
- `-n`, `--no-exec` - do not execute commands from template.

### List templates

```bash
templater list
```

Additional flags:
- `-n`, `--name` - filter templates by name.
- `-c`, `--commands` - list commands of template. Is dependent on `-n` argument.
- `-t`, `--tree` - show file tree of the template.Is dependent on `-n` argument.

### Remove template

```bash
templater delete <template_name>
```

### Edit template metadata

```bash
templater edit <template_name>
```

This will open text editor from your `$EDITOR` variable, or `vim` if its empty.

## TODO / Ideas

- ~~some kind of definition file, that can be used instead of command-line arguments.~~ Done? TODO: fix me gagging everytime I see the code I've wrote to make this work.
- ~~list template file structure~~ Done. TODO: Check if I can remove `print_tar_tree` beast of a function.
