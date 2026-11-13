import setuptools

with open(".version") as f:
    version = f.read().strip()

setuptools.setup(
    name="music-cli",
    version=version,
    python_requires=">=3.12.0",
    author="blissful",
    author_email="blissful@sunsetglow.net",
    license="Apache-2.0",
    entry_points={"console_scripts": ["music = music_cli.__main__:main"]},
    packages=["music_cli"],
    package_data={"music_cli": ["py.typed"]},
    install_requires=[
        "click",
        "music",
        "music-vfs",
        "music-watch",
    ],
)
