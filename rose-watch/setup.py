import setuptools

with open(".version") as f:
    version = f.read().strip()

setuptools.setup(
    name="rose-watch",
    version=version,
    python_requires=">=3.11.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    packages=["rose_watch"],
    package_data={"rose_watch": ["py.typed"]},
    install_requires=[
        "rose",
        "watchdog",
    ],
)
