// Panics with "local read before letrec init"
{r: [1 for a in [1] for b in [[1,2]] for c in b]}
