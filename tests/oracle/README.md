# v4-core quote oracle

The checked-in fixture comes from `Pool.swap` in Uniswap/v4-core commit
`bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6`, compiled with Solidity 0.8.26.
Regeneration was tested with Foundry 1.9.0.

From this directory:

```sh
git clone https://github.com/Uniswap/v4-core.git lib/v4-core
git -C lib/v4-core checkout bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6
git -C lib/v4-core submodule update --init
forge test --match-test testExport -vv
python3 normalize.py
```

Normal Rust tests consume the fixture and require neither Foundry nor an RPC.
Run them from the repository root:

```sh
cargo test --test issue74_oracle
```

The harness initializes a fresh pool for each case and records token deltas,
price, tick, liquidity, total fee and reverts. The matrix includes both swap
directions, exact input/output, directional protocol fees, LP fee boundaries,
initialized tick limits, and empty-liquidity traversal at a 100% fee.

It exercises the core pool library. Hook execution, PoolManager settlement,
token transfers and live deployments are outside this fixture's scope. The
Rust integration test applies each result to static pools and resolved dynamic
pools whose hook address has no permissions.
