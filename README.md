    # SigmaOS-packages-tools

    Package install/remove CLI (captain/gdebi-style)

    Part of the **SigmaOS** sovereign operating-system ecosystem — a multi-repository
    distribution modelled on Linux Mint and Omarchy Linux. See the parent
    [`SigmaOS`](https://github.com/AaryanSinghChauhan09/SigmaOS) repository for the
    full component index.

    ## Upstream inspirations
    - https://github.com/linuxmint/captain
- https://github.com/linuxmint/gdebi

    ## Design principles applied
    - **Fail-Fast**
- **Defensive Programming**
- **Least Astonishment**
- **KISS**
- **Separation of Concerns**
- **Deterministic Testing**

    ## Usage
    Verbs: `list, add, remove, enable, test, parse-apturl`

    ```
    $ sigma-sources list --file sources.list.d/ppa.list
$ sigma-sources add --file repos.list --url https://ppa.launchpadcontent.net/x/y/ubuntu --suite noble --components main
$ sigma-sources remove --file repos.list --match ppa.launchpadcontent.net/x/y
$ sigma-sources enable --file repos.list --match example.com --off
$ sigma-sources test --line "deb [signed-by=/k.gpg] https://example.com/apt noble main"
$ sigma-sources parse-apturl "apt://htop?architecture=amd64"

    ```

    ## Build & test
    ```
    make build
    make test
    ```

    ## Licence
    GNU General Public License v3.0 (FOSS).
