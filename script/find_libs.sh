#!/bin/bash

if [ -z "$1" ]; then
    DIR="."
else
    DIR="$1"
fi

TEMP_DEPS_FILE="$(mktemp)"

find "$DIR" -type f | while read -r file; do
    if file "$file" | grep -E 'ELF.*(executable|shared object)' > /dev/null; then
        ldd "$file" 2>/dev/null | awk '{
            if ($2 == "=>") {
                print $3
            } else if ($1 ~ /\.so/) {
                print $1
            }
        }' | grep "\.so" >> "$TEMP_DEPS_FILE"
    fi
done

sort -u "$TEMP_DEPS_FILE" | grep -v '^$' > unique_dependencies.txt

rm -f "$TEMP_DEPS_FILE"

echo "Unique .so dependencies:"

cat unique_dependencies.txt

rm -f unique_dependencies.txt
