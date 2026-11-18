package main

import (
	"fmt"
	"os"
	"strings"

	_ "github.com/helius-labs/zolana-custom-rings/prover/ring"
	"github.com/helius-labs/zolana/prover/server/app"
	"github.com/helius-labs/zolana/prover/server/logging"
	"github.com/rs/zerolog"
)

func main() {
	command := subcommand(os.Args[1:])
	// Proof errors can contain private witness values.
	private := command == "start" || command == "prove"
	if private {
		zerolog.SetGlobalLevel(zerolog.Disabled)
	}
	if err := app.Run(os.Args); err != nil {
		if private {
			fmt.Fprintf(os.Stderr, "prover %s failed. Check the proving keys and backend configuration.\n", command)
			os.Exit(1)
		}
		logging.Logger().Fatal().Err(err).Msg("App failed.")
	}
}

func subcommand(args []string) string {
	// The prover accepts only boolean flags before the command.
	for _, arg := range args {
		if !strings.HasPrefix(arg, "-") {
			return arg
		}
	}
	return ""
}
