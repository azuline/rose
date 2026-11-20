import setuptools

with open(".version") as f:
    version = f.read().strip()

setuptools.setup(
    name="music-watch",
    version=version,
    python_requires=">=3.12.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    packages=["music_watch"],
    package_data={"music_watch": ["py.typed"]},
    install_requires=[
        "music",
        "watchdog",
    ],
)
