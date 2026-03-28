#include <unistd.h>
#include <stdlib.h> // Mafiya li kebt atoi rani merid

void    ft_putchar(char fdy)
{
    write(1, &fdy, 1);
}

void    ft_putnbr(long long nb)
{
    if (nb > 9)
    {
        ft_putnbr(nb / 10);
    }
    ft_putchar((nb % 10) + '0');
}

int prime(int nb)
{
    if (nb <= 1)
        return 0;
    for (int i = 2; i < nb; i++)
    {
        if (nb % i == 0)
            return 0;
    }
    return 1;
}
int main(int ac, char **av)
{
    if (ac == 2)
    {
        int sum = 0;
        int nb = atoi(av[1]);
        if (!prime(nb))
        {
            write(1, "0\n", 2);
            return 0;
        }
        for (int i = 1; i <= nb; i++)
        {
            if (prime(i))
                sum += i;
        }
        ft_putnbr(sum);
        write(1, "\n", 1);
        return 0;
    }
    write(1, "0\n", 2);
    return 0;
}