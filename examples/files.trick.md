# files

## Find files by name

Recursively search for files matching a pattern.

```sh
find <path> -name "<pattern>" -type f
```

$ path: echo -e ".\n/\n/home\n/etc\n/var/log"

## Find large files

Find files larger than the specified size.
Useful for freeing up disk space.

```sh
find <path> -type f -size +<size> -exec ls -lh {} \; | sort -k5 -h
```

$ size: echo -e "100M\n500M\n1G\n10G"

## Disk usage summary

```sh
du -sh <path>/*  | sort -rh | head -20
```

## Watch a file for changes

```sh
tail -f <file>
```

## Compare two files

Show differences side by side with colors.

```sh
diff --color -u <file1> <file2>
```

## Create a tar archive

Compress a directory into a tar.gz archive.

```sh
tar -czf <archive_name>.tar.gz <directory>
```

## Extract a tar archive

```sh
tar -xzf <archive>
```
