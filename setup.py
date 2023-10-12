import setuptools

with open("version") as f:
    version = f.read()

setuptools.setup(
    name="rose",
    version=version,
    python_requires=">=3.11.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    entry_points={"console_scripts": ["rose = rose.__main__:cli"]},
    packages=setuptools.find_namespace_packages(where="."),
    package_data={"rose": ["*.sql"]},
    install_requires=[
        "click",
        "fuse-python",
        "mutagen",
        "tomli-w",
        "uuid6",
    ],
)
