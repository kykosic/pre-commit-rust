from setuptools import setup

setup(
    name="pre_commit_rust",
    version="0.1.0",
    py_modules=["pre_commit_rust"],
    entry_points={"console_scripts": ["pre_commit_rust=pre_commit_rust:main"]},
)
