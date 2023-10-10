import setuptools

setuptools.setup(
    name="rose",
    version="0.0.0",
    python_requires=">=3.11.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    entry_points={"console_scripts": ["rose = rose.__main__:cli"]},
    packages=setuptools.find_namespace_packages(where="."),
    install_requires=[
        "click",
        "fuse-python",
        "mutagen",
        "uuid6-python",
        "yoyo-migrations",
    ],
)
