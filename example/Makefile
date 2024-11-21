# Number of clients to run
NUM_CLIENTS := 100
IP_LIST := 127.0.0.1:3000,127.0.0.1:3001

.PHONY: load-test

load-test:
	@echo "Starting $(NUM_CLIENTS) clients..."
	@seq 1 $(NUM_CLIENTS) | xargs -n 1 -P $(NUM_CLIENTS) -I {} \
		sh -c 'if [ $$(({})) -eq $(NUM_CLIENTS) ]; then ./target/release/example --mode client --report --ips $(IP_LIST); else ./target/release/example --mode client --ips $(IP_LIST) > /dev/null 2>&1; fi'
	@echo "Load test completed."