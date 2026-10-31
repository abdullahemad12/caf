# Overview

This document contains the different features offered by the CAF and the status of their implementation

## Installation

Currently downloading the compressed file works however, If the node finds itself as a provider then it simply doesn't find the file (logically so because it will probably try to connect to itself through the same port)

#### Tasks

1. Before trying to install the package, check if the package exists and is already installed
2. Periodically, check which package the node can provide and register the node as a provider for the package. If a package no longer locally exists
   stop providing it


## Uninstallation

- Not implemented at all
