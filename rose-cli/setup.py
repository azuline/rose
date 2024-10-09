import setuptools

with open(".version") as f:
    version = f.read().strip()

setuptools.setup(
    name="rose-cli",
    version=version,
    python_requires=">=3.12.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    entry_points={"console_scripts": ["rose = rose_cli.__main__:main"]},
    packages=["rose_cli"],
    package_data={"rose_cli": ["py.typed"]},
    install_requires=[
        "click",
        "rose",
        "rose-vfs",
        "rose-watch",
    ],
)
