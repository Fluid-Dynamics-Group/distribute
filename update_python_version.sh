# parse the `version` number from cargo
CARGO_VERSION=$(grep version Cargo.toml |head -1)
# parse the `version` number from the python version
PYBIND_VERSION=$(grep version pybind/pyproject.toml | head -1)

echo "cargo version is $CARGO_VERSION"
echo "python version is $PYBIND_VERSION"

# swap the python version for the cargo version
sed -i "s/$PYBIND_VERSION/$CARGO_VERSION/g" pybind/pyproject.toml

PYBIND_VERSION_NEW=$(grep version pybind/pyproject.toml | head -1)
echo "updated python version is $PYBIND_VERSION_NEW"
