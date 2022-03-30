# banyan

banyan is an experimental filesystem snapshot/layering tool.

It has an experimental multithreaded queue that directly uses `getdents64`
instead of `readdir`, see `src/util/queue.rs`. It also tries to avoid
copies as much as possible.

Snapshot restore is still a work-in-progress, and the bitstream is not fully
frozen yet, so functionality is not guaranteed! Use at your own risk;
I am not responsible for eaten data.

Current functionality is exposed through a CLI:

```
banyan -r ~/testrepo init
banyan -r ~/testrepo import /path/to/snapshot/
```

## TODO

- snapshot restores + performance tuning
- more user-friendly error handling
- more comprehensive documentation
