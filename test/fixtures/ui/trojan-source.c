// Synthetic fixture: Trojan Source (CVE-2021-42574) pattern.
int main(void) {
    int is_admin = 0;
    /*‮ } ⁦if (is_admin)⁩ ⁦ begin admins only */
        return 1;
    /* end admins only ‮ { ⁦*/
    return 0;
}
