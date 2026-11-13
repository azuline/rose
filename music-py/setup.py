import setuptools

with open("music/.version") as f:
    version = f.read().strip()

setuptools.setup(
    name="music",
    version=version,
    python_requires=">=3.12.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    packages=["music"],
    package_data={"music": ["*.sql", ".version", "py.typed"]},
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
