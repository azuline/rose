import setuptools

with open("rose/.version") as f:
    version = f.read().strip()

setuptools.setup(
    name="rose",
    version=version,
    python_requires=">=3.12.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    packages=["rose"],
    package_data={"rose": ["*.sql", ".version", "py.typed"]},
    install_requires=[
        "appdirs",
        "click",
        "jinja2",
        "llfuse",
        "mutagen",
        "send2trash",
        "tomli-w",
        "uuid6",
    ],
)
