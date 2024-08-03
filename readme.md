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
- `-f`, `--force` - force overwrite existing template.

### Create a project from template

```bash
templater expand <template_name>
```

Additional flags:
- `-a`, `--as` - name of the project. If not provided, name of the project will be the same as the template.
- `-p`, `--path` - path where project will be created. If not provided, project will be created in current directory. Templaters are allways expanded to new, empty directory.
- `-n`, `--no-exec` - do not execute commands from template.

### List templates

```bash
templater list
```

Additional flags:
- `-n`, `--name` - filter templates by name.
- `-c`, `--commands` - list commands of template. Is dependent on `-n` flag.

### Remove template

```bash
templater delete <template_name>
```

### Edit template

```bash
templater edit <template_name>
```
