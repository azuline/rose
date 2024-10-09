import setuptools

with open(".version") as f:
    version = f.read().strip()

setuptools.setup(
    name="rose-vfs",
    version=version,
    python_requires=">=3.12.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    packages=["rose_vfs"],
    package_data={"rose_vfs": ["py.typed"]},
    install_requires=[
        "rose",
        "llfuse",
    ],
)
