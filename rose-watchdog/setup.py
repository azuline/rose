import setuptools

with open(".version") as f:
    version = f.read().strip()

setuptools.setup(
    name="rose-watchdog",
    version=version,
    python_requires=">=3.11.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    packages=["rose_watchdog"],
    package_data={"rose_watchdog": ["py.typed"]},
    install_requires=[
        "rose",
        "watchdog",
    ],
)
