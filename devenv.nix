{ pkgs, lib, config, inputs, ... }:

{
  env = {
    TRACKIT_DB="./db/trackit.db";
  };

  # https://devenv.sh/packages/
  packages = with pkgs;  [ 
    git 
    bacon
    cargo-nextest
    cargo-seek
    cargo-generate
    dbeaver-bin
    lazygit
  ];

  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    channel = "nightly";
    components = ["rustc" "cargo" "clippy" "rust-analyzer"];
  };

  scripts.watcher = {
    exec = ''
      watchexec -c -e rs \
      "cargo clippy && cargo test -- --nocapture"
    '';
    packages = [pkgs.watchexec];
  };

  scripts.wipe-db = {
    exec = ''
      db="''${TRACKIT_DB:-$HOME/.local/share/trackit/trackit.db}"

      if [ -f "$db" ]; then
        echo "Removing: $db"
        rm -f "$db" "$db-wal" "$db-shm"
        echo "Database wiped."
      else
        echo "No database found at: $db"
      fi
    '';
  };

  env.LD_LIBRARY_PATH = [
    pkgs.zlib
  ];

  env = {

  };

  enterShell = ''
    echo "Creates ready to update with 'cargo update'":
    cargo update -n
  '';
}
