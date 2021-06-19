# wsl-get

A command line tool to install any Linux distribution on WSL2.
 
## Requirements

* [Rust compiler](https://www.rust-lang.org/)

* [Docker Desktop](https://www.docker.com/products/docker-desktop)

    This program uses the docker command to get a rootfs tarball that can be used with WSL.

    You can install Docker Desktop via `winget`:

    ```
    > winget install docker
    ```

## Install

```
> cargo install wsl-get
```

## Usage

### Install distribution

```
> wsl-get install <distribution>
```

To find available distributions and versions, search on [Docker Hub](https://hub.docker.com/).
And you can install it by just replacing `docker pull` with `wsl-get install` in the command. For example,

Installing Ubuntu:

```
> wsl-get install ubuntu
```

Installing specified version of distribution:

```
> wsl-get install ubuntu:21.04
```

You can specify the name of installation.

```
> wsl-get install <distribution> <install-name>
```

You can create many instances of same distribution.

```
> wsl-get install ubuntu ubuntu-1
> wsl-get install ubuntu ubuntu-2
> wsl-get install ubuntu ubuntu-3
```

### Uninstall distribution

```
> wsl-get uninstall <distribution>
```

### List installed distributions

```
> wsl-get list
```

Just same as `wsl.exe --list`.

### Set default user of distribution

```
> wsl-get set-default-user <distribution> <username>
```

### Download rootfs tarball

You can download the rootfs tarball in order to install the distribution yourself using the `wsl.exe` command.

```
> wsl-get download <distribution>
```

For more information, please run `wsl-get help`.
